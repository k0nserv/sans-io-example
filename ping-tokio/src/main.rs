use std::net::SocketAddrV4;
use std::time::{Duration, Instant};

use ping_core::{Event, Input, Output, Ping};
use socket::Socket;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::signal;
use tokio::{self, pin};

mod socket;

pub type Result<T> = color_eyre::Result<T>;

#[tokio::main]
async fn main() -> Result<()> {
    let addr: SocketAddrV4 = "8.8.8.8:0".parse()?;
    let mut socket = Socket::new_icmp_v4(addr)?;
    let mut ping = Ping::new(*addr.ip(), Duration::from_millis(1000));
    let ctrl_c = signal::ctrl_c();
    pin!(ctrl_c);

    let mut buf = [0u8; 1500];
    let mut timeout_until = Instant::now();
    let mut last_recv;
    loop {
        let timeout = (timeout_until - Instant::now()).max(Duration::from_millis(1));

        last_recv = tokio::select! {
            _ = &mut ctrl_c => {
                // Ctrl-C
                break;
            }
            _ = tokio::time::sleep(timeout) => {
                Some(Input::Time(Instant::now()))
            }
            read_res = socket.read(&mut buf) => {
                let bytes_read = read_res?;
                Some(Input::Datagram(&buf[..bytes_read], Instant::now()))
            }
        };

        timeout_until = loop {
            let input = last_recv
                .take()
                .unwrap_or_else(|| Input::Time(Instant::now()));
            let output = ping.handle_input(input)?;

            match output {
                Output::Event(event) => {
                    handle_event(event, addr);
                }
                Output::Send(vec) => {
                    socket.write(&vec).await?;
                }
                Output::Timeout(instant) => {
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
