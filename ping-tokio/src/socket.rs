use std::net::SocketAddrV4;
use std::pin::Pin;
use std::task::{ready, Poll};

use socket2::{SockAddr, Socket as RawSocket};
use tokio::io::unix::AsyncFd;
use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf};

use crate::Result;

pub struct Socket {
    addr: SockAddr,
    inner: AsyncFd<RawSocket>,
}

impl Socket {
    pub fn new_icmp_v4(addr: SocketAddrV4) -> Result<Self> {
        let socket = RawSocket::new(
            socket2::Domain::IPV4,
            socket2::Type::RAW,
            Some(socket2::Protocol::ICMPV4),
        )?;
        socket.set_nonblocking(true)?;
        Self::new(socket, SockAddr::from(addr))
    }

    fn new(socket: RawSocket, addr: SockAddr) -> Result<Self> {
        let inner = AsyncFd::new(socket)?;
        Ok(Self { inner, addr })
    }
}

impl AsyncRead for Socket {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut guard = ready!(self.inner.poll_read_ready(cx))?;

        let unfilled = unsafe { buf.inner_mut() };
        match guard.try_io(|inner| inner.get_ref().recv_from(unfilled)) {
            Ok(Ok((len, _))) => {
                buf.advance(len);
                return Poll::Ready(Ok(()));
            }
            Ok(Err(err)) => return Poll::Ready(Err(err)),
            Err(_would_block) => return Poll::Pending,
        }
    }
}

impl AsyncWrite for Socket {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::result::Result<usize, std::io::Error>> {
        let mut guard = ready!(self.inner.poll_write_ready(cx))?;

        match guard.try_io(|inner| inner.get_ref().send_to(buf, &self.addr)) {
            Ok(result) => return Poll::Ready(result),
            Err(_would_block) => return Poll::Pending,
        }
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<std::result::Result<(), std::io::Error>> {
        todo!()
    }
}
