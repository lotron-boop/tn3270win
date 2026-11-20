//! TLS client using Windows' built-in Schannel (no external libraries).
//!
//! The handshake runs with manual certificate validation; afterwards the
//! server certificate is checked against the Windows certificate store
//! (chain, expiry, host name). If Windows doesn't trust it, the
//! certificate's SHA-256 fingerprint is compared with the one the user
//! trusted before for this host; if there is none (or it changed), the UI is
//! asked through a [`NetEvent::Prompt`].
#![allow(non_snake_case, non_camel_case_types, dead_code, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::channel;

use tn3270::net::{NetEvent, Transport, TrustAnswer, TrustPrompt};
use tn3270::sha256::fingerprint;

use crate::settings;
use crate::win32::wide;

// ---- Schannel / SSPI bindings ------------------------------------------------

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SecHandle {
    dwLower: usize,
    dwUpper: usize,
}

#[repr(C)]
struct SecBuffer {
    cbBuffer: u32,
    BufferType: u32,
    pvBuffer: *mut c_void,
}

#[repr(C)]
struct SecBufferDesc {
    ulVersion: u32,
    cBuffers: u32,
    pBuffers: *mut SecBuffer,
}

#[repr(C)]
#[derive(Default)]
struct SecPkgContext_StreamSizes {
    cbHeader: u32,
    cbTrailer: u32,
    cbMaximumMessage: u32,
    cBuffers: u32,
    cbBlockSize: u32,
}

#[repr(C)]
struct SCHANNEL_CRED {
    dwVersion: u32,
    cCreds: u32,
    paCred: *mut c_void,
    hRootStore: *mut c_void,
    cMappers: u32,
    aphMappers: *mut c_void,
    cSupportedAlgs: u32,
    palgSupportedAlgs: *mut u32,
    grbitEnabledProtocols: u32,
    dwMinimumCipherStrength: u32,
    dwMaximumCipherStrength: u32,
    dwSessionLifespan: u32,
    dwFlags: u32,
    dwCredFormat: u32,
}

#[repr(C)]
struct CRYPT_BLOB {
    cbData: u32,
    pbData: *mut u8,
}

#[repr(C)]
struct CRYPT_ALGORITHM_IDENTIFIER {
    pszObjId: *mut i8,
    Parameters: CRYPT_BLOB,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FILETIME {
    dwLowDateTime: u32,
    dwHighDateTime: u32,
}

/// The leading fields of CERT_INFO (all we read).
#[repr(C)]
struct CERT_INFO_HEAD {
    dwVersion: u32,
    SerialNumber: CRYPT_BLOB,
    SignatureAlgorithm: CRYPT_ALGORITHM_IDENTIFIER,
    Issuer: CRYPT_BLOB,
    NotBefore: FILETIME,
    NotAfter: FILETIME,
}

#[repr(C)]
struct CERT_CONTEXT {
    dwCertEncodingType: u32,
    pbCertEncoded: *mut u8,
    cbCertEncoded: u32,
    pCertInfo: *mut CERT_INFO_HEAD,
    hCertStore: *mut c_void,
}

#[repr(C)]
struct CERT_ENHKEY_USAGE {
    cUsageIdentifier: u32,
    rgpszUsageIdentifier: *mut *mut i8,
}

#[repr(C)]
struct CERT_USAGE_MATCH {
    dwType: u32,
    Usage: CERT_ENHKEY_USAGE,
}

#[repr(C)]
struct CERT_CHAIN_PARA {
    cbSize: u32,
    RequestedUsage: CERT_USAGE_MATCH,
}

#[repr(C)]
struct SSL_EXTRA_CERT_CHAIN_POLICY_PARA {
    cbSize: u32,
    dwAuthType: u32,
    fdwChecks: u32,
    pwszServerName: *mut u16,
}

#[repr(C)]
struct CERT_CHAIN_POLICY_PARA {
    cbSize: u32,
    dwFlags: u32,
    pvExtraPolicyPara: *mut c_void,
}

#[repr(C)]
struct CERT_CHAIN_POLICY_STATUS {
    cbSize: u32,
    dwError: u32,
    lChainIndex: i32,
    lElementIndex: i32,
    pvExtraPolicyStatus: *mut c_void,
}

const SECPKG_CRED_OUTBOUND: u32 = 2;
const SCHANNEL_CRED_VERSION: u32 = 4;
const SCH_CRED_MANUAL_CRED_VALIDATION: u32 = 0x0000_0008;
const SCH_CRED_NO_DEFAULT_CREDS: u32 = 0x0000_0010;
const SCH_USE_STRONG_CRYPTO: u32 = 0x0040_0000;

const ISC_REQ_REPLAY_DETECT: u32 = 0x0000_0004;
const ISC_REQ_SEQUENCE_DETECT: u32 = 0x0000_0008;
const ISC_REQ_CONFIDENTIALITY: u32 = 0x0000_0010;
const ISC_REQ_ALLOCATE_MEMORY: u32 = 0x0000_0100;
const ISC_REQ_EXTENDED_ERROR: u32 = 0x0000_4000;
const ISC_REQ_STREAM: u32 = 0x0000_8000;
const ISC_REQ_MANUAL_CRED_VALIDATION: u32 = 0x0008_0000;
const ISC_FLAGS: u32 = ISC_REQ_REPLAY_DETECT | ISC_REQ_SEQUENCE_DETECT | ISC_REQ_CONFIDENTIALITY
    | ISC_REQ_ALLOCATE_MEMORY | ISC_REQ_EXTENDED_ERROR | ISC_REQ_STREAM | ISC_REQ_MANUAL_CRED_VALIDATION;

const SECBUFFER_VERSION: u32 = 0;
const SECBUFFER_EMPTY: u32 = 0;
const SECBUFFER_DATA: u32 = 1;
const SECBUFFER_TOKEN: u32 = 2;
const SECBUFFER_EXTRA: u32 = 5;
const SECBUFFER_STREAM_TRAILER: u32 = 6;
const SECBUFFER_STREAM_HEADER: u32 = 7;

const SEC_E_OK: i32 = 0;
const SEC_I_CONTINUE_NEEDED: i32 = 0x0009_0312;
const SEC_I_CONTEXT_EXPIRED: i32 = 0x0009_0317;
const SEC_I_INCOMPLETE_CREDENTIALS: i32 = 0x0009_0320;
const SEC_I_RENEGOTIATE: i32 = 0x0009_0321;
const SEC_E_INCOMPLETE_MESSAGE: i32 = 0x8009_0318_u32 as i32;

const SECPKG_ATTR_STREAM_SIZES: u32 = 4;
const SECPKG_ATTR_REMOTE_CERT_CONTEXT: u32 = 0x53;

const USAGE_MATCH_TYPE_AND: u32 = 0;
const AUTHTYPE_SERVER: u32 = 2;
const CERT_CHAIN_POLICY_SSL: *const i8 = 4 as *const i8;
const CERT_NAME_SIMPLE_DISPLAY_TYPE: u32 = 4;
const CERT_NAME_ISSUER_FLAG: u32 = 1;

#[cfg_attr(windows, link(name = "secur32"))]
extern "system" {
    fn AcquireCredentialsHandleW(
        principal: *const u16, package: *const u16, cred_use: u32, logon_id: *mut c_void,
        auth_data: *mut c_void, get_key_fn: *mut c_void, get_key_arg: *mut c_void,
        cred: *mut SecHandle, expiry: *mut i64,
    ) -> i32;
    fn InitializeSecurityContextW(
        cred: *mut SecHandle, context: *mut SecHandle, target: *const u16, req: u32, reserved1: u32,
        target_rep: u32, input: *mut SecBufferDesc, reserved2: u32, new_context: *mut SecHandle,
        output: *mut SecBufferDesc, attrs: *mut u32, expiry: *mut i64,
    ) -> i32;
    fn DeleteSecurityContext(context: *mut SecHandle) -> i32;
    fn FreeCredentialsHandle(cred: *mut SecHandle) -> i32;
    fn FreeContextBuffer(buf: *mut c_void) -> i32;
    fn QueryContextAttributesW(context: *mut SecHandle, attr: u32, buf: *mut c_void) -> i32;
    fn EncryptMessage(context: *mut SecHandle, qop: u32, msg: *mut SecBufferDesc, seq: u32) -> i32;
    fn DecryptMessage(context: *mut SecHandle, msg: *mut SecBufferDesc, seq: u32, qop: *mut u32) -> i32;
}

#[cfg_attr(windows, link(name = "crypt32"))]
extern "system" {
    fn CertGetCertificateChain(
        engine: *mut c_void, cert: *const CERT_CONTEXT, time: *mut FILETIME, store: *mut c_void,
        para: *mut CERT_CHAIN_PARA, flags: u32, reserved: *mut c_void, chain: *mut *const c_void,
    ) -> i32;
    fn CertVerifyCertificateChainPolicy(
        policy: *const i8, chain: *const c_void, para: *mut CERT_CHAIN_POLICY_PARA,
        status: *mut CERT_CHAIN_POLICY_STATUS,
    ) -> i32;
    fn CertFreeCertificateChain(chain: *const c_void);
    fn CertFreeCertificateContext(cert: *const CERT_CONTEXT) -> i32;
    fn CertGetNameStringW(
        cert: *const CERT_CONTEXT, kind: u32, flags: u32, type_para: *mut c_void, name: *mut u16, len: u32,
    ) -> u32;
}

fn sec_err(what: &str, status: i32) -> String {
    format!("TLS {what} failed (0x{:08X}). {}", status as u32, explain(status as u32))
}

fn explain(code: u32) -> &'static str {
    match code {
        0x8009_0308 => "The host's TLS reply was not understood.",
        0x8009_0331 | 0x8009_0302 => "The host and Windows have no TLS version or cipher in common.",
        0x8009_0327 => "The host asked for a client certificate.",
        0x8009_0326 => "The host sent an unexpected TLS message.",
        0x8009_0322 => "The host's certificate name does not match.",
        _ => "",
    }
}

// ---- the stream ------------------------------------------------------------------

pub struct TlsStream {
    tcp: TcpStream,
    cred: SecHandle,
    ctx: SecHandle,
    sizes: SecPkgContext_StreamSizes,
    /// Received but not yet decrypted bytes.
    enc: Vec<u8>,
    /// Decrypted bytes not yet handed out.
    plain: Vec<u8>,
    eof: bool,
}

// The handles are plain numbers owned by this stream; it is only ever used
// from one thread at a time (the connection's I/O thread).
unsafe impl Send for TlsStream {}

impl Drop for TlsStream {
    fn drop(&mut self) {
        unsafe {
            DeleteSecurityContext(&mut self.ctx);
            FreeCredentialsHandle(&mut self.cred);
        }
    }
}

fn read_more(tcp: &mut TcpStream, buf: &mut Vec<u8>) -> io::Result<usize> {
    let mut tmp = [0u8; 16 * 1024];
    let n = Read::read(tcp, &mut tmp)?;
    buf.extend_from_slice(&tmp[..n]);
    Ok(n)
}

fn send_token(tcp: &mut TcpStream, out: &SecBuffer) -> io::Result<()> {
    if out.cbBuffer > 0 && !out.pvBuffer.is_null() {
        let token = unsafe { std::slice::from_raw_parts(out.pvBuffer as *const u8, out.cbBuffer as usize) };
        let r = Write::write_all(tcp, token);
        unsafe { FreeContextBuffer(out.pvBuffer) };
        r?;
    }
    Ok(())
}

impl TlsStream {
    /// Run the client handshake over `tcp` (blocking).
    pub fn handshake(mut tcp: TcpStream, host: &str) -> Result<TlsStream, String> {
        let target = wide(host);
        let mut cred = SecHandle::default();
        let mut ctx = SecHandle::default();
        let mut expiry = 0i64;
        let mut sc = SCHANNEL_CRED {
            dwVersion: SCHANNEL_CRED_VERSION,
            cCreds: 0,
            paCred: std::ptr::null_mut(),
            hRootStore: std::ptr::null_mut(),
            cMappers: 0,
            aphMappers: std::ptr::null_mut(),
            cSupportedAlgs: 0,
            palgSupportedAlgs: std::ptr::null_mut(),
            grbitEnabledProtocols: 0, // Windows' defaults (TLS 1.2 with this credential type)
            dwMinimumCipherStrength: 0,
            dwMaximumCipherStrength: 0,
            dwSessionLifespan: 0,
            dwFlags: SCH_CRED_MANUAL_CRED_VALIDATION | SCH_CRED_NO_DEFAULT_CREDS | SCH_USE_STRONG_CRYPTO,
            dwCredFormat: 0,
        };
        let package = wide("Microsoft Unified Security Protocol Provider");
        let status = unsafe {
            AcquireCredentialsHandleW(std::ptr::null(), package.as_ptr(), SECPKG_CRED_OUTBOUND,
                std::ptr::null_mut(), &mut sc as *mut _ as *mut c_void, std::ptr::null_mut(),
                std::ptr::null_mut(), &mut cred, &mut expiry)
        };
        if status != SEC_E_OK {
            return Err(sec_err("setup", status));
        }
        // From here on, `stream` owns the handles and frees them on error.
        let mut stream = TlsStream {
            tcp: tcp.try_clone().map_err(|e| e.to_string())?,
            cred, ctx, sizes: Default::default(), enc: Vec::new(), plain: Vec::new(), eof: false,
        };

        let mut attrs = 0u32;
        let mut first = true;
        let mut inbuf: Vec<u8> = Vec::new();
        loop {
            let mut in_bufs = [
                SecBuffer { cbBuffer: inbuf.len() as u32, BufferType: SECBUFFER_TOKEN, pvBuffer: inbuf.as_mut_ptr() as *mut c_void },
                SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_EMPTY, pvBuffer: std::ptr::null_mut() },
            ];
            let mut in_desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 2, pBuffers: in_bufs.as_mut_ptr() };
            let mut out_bufs = [SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_TOKEN, pvBuffer: std::ptr::null_mut() }];
            let mut out_desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 1, pBuffers: out_bufs.as_mut_ptr() };
            let status = unsafe {
                InitializeSecurityContextW(
                    &mut stream.cred,
                    if first { std::ptr::null_mut() } else { &mut stream.ctx },
                    target.as_ptr(), ISC_FLAGS, 0, 0,
                    if first { std::ptr::null_mut() } else { &mut in_desc },
                    0, &mut ctx, &mut out_desc, &mut attrs, &mut expiry,
                )
            };
            if first {
                stream.ctx = ctx;
                first = false;
            }
            if status == SEC_E_INCOMPLETE_MESSAGE {
                match read_more(&mut tcp, &mut inbuf) {
                    Ok(0) => return Err("The host closed the connection during the TLS handshake. Is this port really TLS?".into()),
                    Ok(_) => continue,
                    Err(e) => return Err(format!("TLS handshake: {e}")),
                }
            }
            send_token(&mut tcp, &out_bufs[0]).map_err(|e| format!("TLS handshake: {e}"))?;
            if status == SEC_I_INCOMPLETE_CREDENTIALS {
                // The host asked for a client certificate; carry on without
                // one, giving Schannel the same input again.
                continue;
            }
            if status != SEC_E_OK && status != SEC_I_CONTINUE_NEEDED {
                return Err(sec_err("handshake", status));
            }
            // Keep any bytes Schannel didn't consume.
            let leftover = if in_bufs[1].BufferType == SECBUFFER_EXTRA && in_bufs[1].cbBuffer > 0 {
                let n = in_bufs[1].cbBuffer as usize;
                inbuf[inbuf.len() - n..].to_vec()
            } else {
                Vec::new()
            };
            inbuf = leftover;
            if status == SEC_E_OK {
                break;
            }
            if inbuf.is_empty() {
                match read_more(&mut tcp, &mut inbuf) {
                    Ok(0) => return Err("The host closed the connection during the TLS handshake. Is this port really TLS?".into()),
                    Ok(_) => {}
                    Err(e) => return Err(format!("TLS handshake: {e}")),
                }
            }
        }
        stream.enc = inbuf; // application data that arrived with the last handshake message
        let status = unsafe {
            QueryContextAttributesW(&mut stream.ctx, SECPKG_ATTR_STREAM_SIZES, &mut stream.sizes as *mut _ as *mut c_void)
        };
        if status != SEC_E_OK {
            return Err(sec_err("setup", status));
        }
        Ok(stream)
    }

    /// Details of the host's certificate.
    pub fn peer_certificate(&mut self, host: &str) -> Result<PeerCert, String> {
        let mut cert: *const CERT_CONTEXT = std::ptr::null();
        let status = unsafe {
            QueryContextAttributesW(&mut self.ctx, SECPKG_ATTR_REMOTE_CERT_CONTEXT, &mut cert as *mut _ as *mut c_void)
        };
        if status != SEC_E_OK || cert.is_null() {
            return Err(sec_err("certificate check", status));
        }
        let info = unsafe {
            let c = &*cert;
            let der = std::slice::from_raw_parts(c.pbCertEncoded, c.cbCertEncoded as usize);
            let not_after = (*c.pCertInfo).NotAfter;
            PeerCert {
                subject: cert_name(cert, 0),
                issuer: cert_name(cert, CERT_NAME_ISSUER_FLAG),
                expires: filetime_to_date(((not_after.dwHighDateTime as u64) << 32) | not_after.dwLowDateTime as u64),
                fingerprint: fingerprint(der),
                problem: verify(cert, host),
            }
        };
        unsafe { CertFreeCertificateContext(cert) };
        Ok(info)
    }

    fn decrypt_some(&mut self) -> io::Result<bool> {
        if self.enc.is_empty() {
            return Ok(false);
        }
        let mut bufs = [
            SecBuffer { cbBuffer: self.enc.len() as u32, BufferType: SECBUFFER_DATA, pvBuffer: self.enc.as_mut_ptr() as *mut c_void },
            SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_EMPTY, pvBuffer: std::ptr::null_mut() },
            SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_EMPTY, pvBuffer: std::ptr::null_mut() },
            SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_EMPTY, pvBuffer: std::ptr::null_mut() },
        ];
        let mut desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 4, pBuffers: bufs.as_mut_ptr() };
        let status = unsafe { DecryptMessage(&mut self.ctx, &mut desc, 0, std::ptr::null_mut()) };
        match status {
            SEC_E_INCOMPLETE_MESSAGE => return Ok(false),
            SEC_I_CONTEXT_EXPIRED => {
                self.eof = true; // the host sent close_notify
                self.enc.clear();
                return Ok(false);
            }
            SEC_E_OK => {}
            SEC_I_RENEGOTIATE => {
                return Err(io::Error::other("The host asked to renegotiate TLS, which is not supported."));
            }
            s => return Err(io::Error::other(sec_err("decrypt", s))),
        }
        let mut extra: Option<Vec<u8>> = None;
        for b in &bufs {
            if b.BufferType == SECBUFFER_DATA && b.cbBuffer > 0 {
                let data = unsafe { std::slice::from_raw_parts(b.pvBuffer as *const u8, b.cbBuffer as usize) };
                self.plain.extend_from_slice(data);
            } else if b.BufferType == SECBUFFER_EXTRA && b.cbBuffer > 0 {
                let data = unsafe { std::slice::from_raw_parts(b.pvBuffer as *const u8, b.cbBuffer as usize) };
                extra = Some(data.to_vec());
            }
        }
        self.enc = extra.unwrap_or_default();
        Ok(true)
    }
}

impl Transport for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            if !self.plain.is_empty() {
                let n = buf.len().min(self.plain.len());
                buf[..n].copy_from_slice(&self.plain[..n]);
                self.plain.drain(..n);
                return Ok(n);
            }
            if self.eof {
                return Ok(0);
            }
            // Decrypt whatever complete records we already have.
            if self.decrypt_some()? {
                continue;
            }
            if self.eof {
                return Ok(0);
            }
            // Need more bytes from the network. A timeout passes up to the
            // caller; the partial record stays in `enc`.
            let n = read_more(&mut self.tcp, &mut self.enc)?;
            if n == 0 {
                return Ok(0);
            }
        }
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        let (h, t) = (self.sizes.cbHeader as usize, self.sizes.cbTrailer as usize);
        let max = (self.sizes.cbMaximumMessage as usize).max(1);
        for chunk in data.chunks(max) {
            let mut msg = vec![0u8; h + chunk.len() + t];
            msg[h..h + chunk.len()].copy_from_slice(chunk);
            let base = msg.as_mut_ptr();
            let mut bufs = [
                SecBuffer { cbBuffer: h as u32, BufferType: SECBUFFER_STREAM_HEADER, pvBuffer: base as *mut c_void },
                SecBuffer { cbBuffer: chunk.len() as u32, BufferType: SECBUFFER_DATA, pvBuffer: unsafe { base.add(h) } as *mut c_void },
                SecBuffer { cbBuffer: t as u32, BufferType: SECBUFFER_STREAM_TRAILER, pvBuffer: unsafe { base.add(h + chunk.len()) } as *mut c_void },
                SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_EMPTY, pvBuffer: std::ptr::null_mut() },
            ];
            let mut desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 4, pBuffers: bufs.as_mut_ptr() };
            let status = unsafe { EncryptMessage(&mut self.ctx, 0, &mut desc, 0) };
            if status != SEC_E_OK {
                return Err(io::Error::other(sec_err("encrypt", status)));
            }
            let total = (bufs[0].cbBuffer + bufs[1].cbBuffer + bufs[2].cbBuffer) as usize;
            Write::write_all(&mut self.tcp, &msg[..total])?;
        }
        Ok(())
    }
}

// ---- certificate checks -------------------------------------------------------------

pub struct PeerCert {
    pub subject: String,
    pub issuer: String,
    pub expires: String,
    pub fingerprint: String,
    /// Why Windows doesn't trust it; `None` if it does.
    pub problem: Option<String>,
}

unsafe fn cert_name(cert: *const CERT_CONTEXT, flags: u32) -> String {
    let mut buf = [0u16; 256];
    let n = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, flags, std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
    String::from_utf16_lossy(&buf[..(n as usize).saturating_sub(1)])
}

/// Check the chain and host name with Windows' SSL policy.
unsafe fn verify(cert: *const CERT_CONTEXT, host: &str) -> Option<String> {
    let mut para = CERT_CHAIN_PARA {
        cbSize: std::mem::size_of::<CERT_CHAIN_PARA>() as u32,
        RequestedUsage: CERT_USAGE_MATCH {
            dwType: USAGE_MATCH_TYPE_AND,
            Usage: CERT_ENHKEY_USAGE { cUsageIdentifier: 0, rgpszUsageIdentifier: std::ptr::null_mut() },
        },
    };
    let mut chain: *const c_void = std::ptr::null();
    if CertGetCertificateChain(std::ptr::null_mut(), cert, std::ptr::null_mut(), (*cert).hCertStore,
        &mut para, 0, std::ptr::null_mut(), &mut chain) == 0 || chain.is_null()
    {
        return Some("Windows could not build a certificate chain.".into());
    }
    let mut name = wide(host);
    let mut ssl = SSL_EXTRA_CERT_CHAIN_POLICY_PARA {
        cbSize: std::mem::size_of::<SSL_EXTRA_CERT_CHAIN_POLICY_PARA>() as u32,
        dwAuthType: AUTHTYPE_SERVER,
        fdwChecks: 0,
        pwszServerName: name.as_mut_ptr(),
    };
    let mut policy = CERT_CHAIN_POLICY_PARA {
        cbSize: std::mem::size_of::<CERT_CHAIN_POLICY_PARA>() as u32,
        dwFlags: 0,
        pvExtraPolicyPara: &mut ssl as *mut _ as *mut c_void,
    };
    let mut status = CERT_CHAIN_POLICY_STATUS {
        cbSize: std::mem::size_of::<CERT_CHAIN_POLICY_STATUS>() as u32,
        dwError: 0, lChainIndex: 0, lElementIndex: 0, pvExtraPolicyStatus: std::ptr::null_mut(),
    };
    let ok = CertVerifyCertificateChainPolicy(CERT_CHAIN_POLICY_SSL, chain, &mut policy, &mut status);
    CertFreeCertificateChain(chain);
    if ok == 0 {
        return Some("Windows could not check the certificate.".into());
    }
    match status.dwError {
        0 => None,
        0x800B_0109 => Some("It is self-signed or issued by an authority Windows doesn't trust.".into()),
        0x800B_010F => Some(format!("It was not issued for \u{201C}{host}\u{201D}.")),
        0x800B_0101 => Some("It has expired or is not yet valid.".into()),
        0x800B_010C => Some("It has been revoked.".into()),
        0x800B_0110 => Some("It is not meant for TLS servers.".into()),
        e => Some(format!("Windows rejected it (0x{e:08X}).")),
    }
}

/// FILETIME (100 ns ticks since 1601-01-01 UTC) as YYYY-MM-DD.
pub fn filetime_to_date(ticks: u64) -> String {
    let days_since_1601 = (ticks / 10_000_000 / 86_400) as i64;
    let days = days_since_1601 - 134_774; // 1601-01-01 to 1970-01-01
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// The connection wrapper for a TLS session: handshake, then trust check.
pub fn wrapper(host: String, port: u16) -> tn3270::net::Wrapper {
    Box::new(move |tcp, emit| {
        let mut tls = TlsStream::handshake(tcp, &host)?;
        let cert = tls.peer_certificate(&host)?;
        if let Some(problem) = cert.problem {
            let saved = settings::trusted_fingerprint(&host, port);
            if saved.as_deref() != Some(cert.fingerprint.as_str()) {
                let (tx, rx) = channel();
                emit(NetEvent::Prompt(TrustPrompt {
                    host: host.clone(), port,
                    subject: cert.subject, issuer: cert.issuer, expires: cert.expires,
                    fingerprint: cert.fingerprint.clone(), problem,
                    changed: saved.is_some(), reply: tx,
                }));
                match rx.recv() {
                    Ok(TrustAnswer::Always) => {
                        if let Err(e) = settings::trust_fingerprint(&host, port, &cert.fingerprint) {
                            emit(NetEvent::Secured(format!("TLS (could not remember the certificate: {e})")));
                        }
                    }
                    Ok(TrustAnswer::Once) => {}
                    Ok(TrustAnswer::Reject) | Err(_) => return Err("Connection cancelled: the certificate was not trusted.".into()),
                }
            }
        }
        emit(NetEvent::Secured("TLS".into()));
        Ok(Box::new(tls) as Box<dyn Transport>)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_from_filetime() {
        // 1970-01-01 is 116444736000000000 ticks.
        assert_eq!(filetime_to_date(116_444_736_000_000_000), "1970-01-01");
        // 2036-11-18 00:00 UTC = 2110579200 s after 1970.
        assert_eq!(filetime_to_date(116_444_736_000_000_000 + 2_110_579_200 * 10_000_000), "2036-11-18");
        assert_eq!(filetime_to_date(116_444_736_000_000_000 + 951_782_400 * 10_000_000), "2000-02-29");
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn struct_sizes_match_windows_sdk() {
        use std::mem::size_of;
        assert_eq!(size_of::<SecHandle>(), 16);
        assert_eq!(size_of::<SecBuffer>(), 16);
        assert_eq!(size_of::<SecBufferDesc>(), 16);
        assert_eq!(size_of::<SecPkgContext_StreamSizes>(), 20);
        assert_eq!(size_of::<SCHANNEL_CRED>(), 80);
        assert_eq!(size_of::<CERT_CONTEXT>(), 40);
        assert_eq!(std::mem::offset_of!(CERT_INFO_HEAD, NotAfter), 72);
        assert_eq!(size_of::<CERT_CHAIN_PARA>(), 32); // cbSize, padding, then an 8-aligned CERT_USAGE_MATCH
        assert_eq!(size_of::<SSL_EXTRA_CERT_CHAIN_POLICY_PARA>(), 24);
        assert_eq!(size_of::<CERT_CHAIN_POLICY_PARA>(), 16);
        assert_eq!(size_of::<CERT_CHAIN_POLICY_STATUS>(), 24);
    }
}
