//! SSL/TLS implementation backed by rustls 0.23（常开，证书由基础功能侧注入）。

#[path = "安全层/安全流.rs"] pub(crate) mod 安全流;
pub(crate) use self::安全流::RustlsContext as SslContextImpl;
pub(crate) use self::安全流::RustlsStream as SslStream;
