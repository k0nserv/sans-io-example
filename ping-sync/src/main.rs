use std::mem::MaybeUninit;
use std::net::SocketAddrV4;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use color_eyre::eyre::Result;
use socket2::{Domain, Protocol, SockAddr, Socket};

use ping_core::{self as ping, BasicContext, Event, Input, Ping};

static STOP: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    let socket = create_socket()?;
    let addr: SocketAddrV4 = "8.8.8.8:0".parse()?;
    let sock_addr = SockAddr::from(addr);
    let mut context = BasicContext::default();
    let mut ping = Ping::new(*addr.ip(), Duration::from_millis(1000));
    ctrlc::set_handler(move || {
        STOP.store(true, Ordering::Relaxed);
    })?;

    let mut buf: [MaybeUninit<u8>; 1500] = [MaybeUninit::uninit(); 1500];
    let mut timeout_until = Instant::now();
    let mut last_recv: Option<Input<'_>>;
    while !STOP.load(Ordering::Relaxed) {
        let timeout = (timeout_until - Instant::now()).max(Duration::from_millis(1));

        socket.set_read_timeout(Some(timeout))?;
        last_recv = match socket.recv_from(&mut buf) {
            Ok((len, _)) => {
                // SAFETY: We read at least `len` bytes into `buf`, so it's safe to read from it.
                let read = unsafe { slice_assume_init_ref(&buf[..len]) };
                Some(Input::Datagram(read, Instant::now()))
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    Some(Input::Time(Instant::now()))
                } else {
                    return Err(e.into());
                }
            }
        };

        timeout_until = loop {
            let input = last_recv
                .take()
                .unwrap_or_else(|| Input::Time(Instant::now()));
            let output = ping.handle_input(input, &mut context)?;

            match output {
                ping::Output::Event(event) => {
                    handle_event(event, addr);
                }
                ping::Output::Send(vec) => {
                    socket.send_to(vec, &sock_addr)?;
                }
                ping::Output::Timeout(instant) => {
                    break instant;
                }
            }
        };
    }

    Ok(())
}

fn handle_event(event: Event, addr: SocketAddrV4) {
    match event {
        Event::Response { seq_num, rtt } => match rtt {
            Some(rtt) => {
                let rtt_ms = rtt.as_secs_f64() * 1000.0;
                println!("Reply from {addr} icmp_seq={seq_num} time={rtt_ms:.2}ms");
            }
            None => {
                println!("Request timeout for icmp_seq={seq_num} after 1s");
            }
        },
    }
}

fn create_socket() -> Result<Socket> {
    let socket = Socket::new(Domain::IPV4, socket2::Type::RAW, Some(Protocol::ICMPV4))?;
    Ok(socket)
}

// Stolen from std since it's still nightly only
/// # Safety
///  The caller must guarantee that `slice` is initialized.
#[inline(always)]
pub const unsafe fn slice_assume_init_ref<T>(slice: &[MaybeUninit<T>]) -> &[T] {
    // SAFETY: casting `slice` to a `*const [T]` is safe since the caller guarantees that
    // `slice` is initialized, and `MaybeUninit` is guaranteed to have the same layout as `T`.
    // The pointer obtained is valid since it refers to memory owned by `slice` which is a
    // reference and thus guaranteed to be valid for reads.
    unsafe { &*(slice as *const [MaybeUninit<T>] as *const [T]) }
}
