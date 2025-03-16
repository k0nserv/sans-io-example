use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use pnet::packet::icmp::echo_reply::EchoReplyPacket;
use pnet::packet::icmp::echo_request::MutableEchoRequestPacket;
use pnet::packet::icmp::{checksum, IcmpCode, IcmpPacket, IcmpType, MutableIcmpPacket};
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::Packet as _;
use thiserror::Error;

pub enum Input<'p> {
    /// Some incoming datagram to handle
    Datagram(&'p [u8], Instant),
    /// Time input (periodic to drive time forward)
    Time(Instant),
}

#[derive(Debug)]
pub enum Output<'b> {
    /// An event that happened
    Event(Event),
    /// Some data to send
    Send(&'b [u8]),
    /// We don't need to do anything until more data is received or the timeout is reached
    Timeout(Instant),
}

#[derive(Debug)]
pub enum Event {
    /// The result a single ping request
    Response {
        /// Sequence number of the ping.
        seq_num: u16,
        /// Time it took to get a response if we got one before the timeout.
        rtt: Option<Duration>,
    },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid packet")]
    InvalidPacket,

    #[error("Not an ICMP packet")]
    NotICMP,

    #[error("Unhandled ICMP type: {0}")]
    UnhandledICMPType(u8),

    #[error("Unexpected identifier")]
    UnexpectedIdentifier,

    #[error("Incorrect target should be {0}, but was {1}")]
    IncorrectTarget(Ipv4Addr, Ipv4Addr),
}

const PING_FREQUENCY: Duration = Duration::from_secs(1);

pub struct Ping {
    target: Ipv4Addr,
    identifier: u16,
    timeout: Duration,
    next_seq_num: u16,
    last_send: Option<Instant>,
    requests: [Option<Request>; 10],
}

#[derive(Copy, Clone, Eq, PartialEq)]
struct Request {
    sent_at: Instant,
    expires_at: Instant,
    seq_num: u16,
}

pub trait Context {
    fn buffer(&mut self, size: usize) -> &mut [u8];
}

impl Ping {
    pub fn new(target: Ipv4Addr, timeout: Duration) -> Self {
        Self {
            target,
            identifier: 0x1337,
            timeout,
            next_seq_num: 0,
            last_send: None,
            requests: [None; 10],
        }
    }

    pub fn handle_input<'s: 'b, 'b>(
        &'s mut self,
        input: Input,
        context: &'b mut impl Context,
    ) -> Result<Output<'b>, Error> {
        match input {
            Input::Datagram(buf, now) => self.hande_datagram(buf, now),
            Input::Time(now) => Ok(self.handle_timeout(now, context)),
        }
    }

    fn hande_datagram<'b>(&'b mut self, buf: &[u8], now: Instant) -> Result<Output<'b>, Error> {
        // We get the IP header and then the ICMP message
        let ip_packet = Ipv4Packet::new(buf).ok_or(Error::InvalidPacket)?;
        if ip_packet.get_source() != self.target {
            return Err(Error::IncorrectTarget(
                self.target,
                ip_packet.get_destination(),
            ));
        }
        if ip_packet.get_next_level_protocol().0 != 1 {
            return Err(Error::NotICMP);
        }
        let icmp_packet = IcmpPacket::new(ip_packet.payload()).ok_or(Error::InvalidPacket)?;
        let echo_reply = EchoReplyPacket::new(ip_packet.payload())
            .ok_or(Error::UnhandledICMPType(icmp_packet.get_icmp_type().0))?;

        if echo_reply.get_identifier() != self.identifier {
            return Err(Error::UnexpectedIdentifier);
        }

        if let Some(request) = self.claim_request(echo_reply.get_sequence_number()) {
            let rtt = Some(now - request.sent_at);

            return Ok(Output::Event(Event::Response {
                seq_num: echo_reply.get_sequence_number(),
                rtt,
            }));
        }

        // Maybe an error since we didn't find the seq_num in the deadlines
        Ok(Output::Timeout(now + self.timeout))
    }

    fn handle_timeout<'b>(&mut self, now: Instant, context: &'b mut impl Context) -> Output<'b> {
        let next_send = self.next_send_at(now);
        if next_send <= now {
            return self.emit_ping(now, context);
        }

        let Some((idx, request)) = self.next_expiry() else {
            return Output::Timeout(next_send);
        };

        if request.expires_at > now {
            return Output::Timeout(request.expires_at.min(next_send));
        }

        self.clear_request(idx);

        Output::Event(Event::Response {
            seq_num: request.seq_num,
            rtt: None,
        })
    }

    fn next_send_at(&self, now: Instant) -> Instant {
        self.last_send
            .map(|last_send| last_send + PING_FREQUENCY)
            .unwrap_or(now)
    }

    fn emit_ping<'b>(&mut self, now: Instant, context: &'b mut impl Context) -> Output<'b> {
        let seq_num = self.next_seq_num;
        self.next_seq_num += 1;
        self.last_send = Some(now);
        self.requests[seq_num as usize % self.requests.len()] = Some(Request {
            sent_at: now,
            expires_at: now + self.timeout,
            seq_num,
        });
        let buf = context.buffer(8);
        {
            let mut packet = MutableEchoRequestPacket::new(buf).expect("buffer is big enough");
            packet.set_icmp_type(IcmpType(8));
            packet.set_icmp_code(IcmpCode(0));
            packet.set_identifier(self.identifier);
            packet.set_sequence_number(seq_num);
        }
        // Now we have a buffer with the ICMP packet, compute and set the checksum
        let mut icmp_packet = MutableIcmpPacket::new(buf).expect("buffer is big enough");
        let checksum = checksum(&icmp_packet.to_immutable());
        icmp_packet.set_checksum(checksum);

        Output::Send(buf)
    }

    fn claim_request(&mut self, seq_num: u16) -> Option<Request> {
        let idx = self
            .requests
            .iter()
            .position(|r| r.map(|r| r.seq_num == seq_num).unwrap_or(false))?;

        self.requests[idx].take()
    }

    fn next_expiry(&mut self) -> Option<(usize, Request)> {
        self.requests
            .iter_mut()
            .enumerate()
            .filter_map(|(i, r)| r.and_then(|r| Some((i, r))))
            .min_by(|(_, a), (_, b)| a.expires_at.cmp(&b.expires_at))
    }

    fn clear_request(&mut self, idx: usize) {
        self.requests[idx] = None;
    }
}

impl<'b> Output<'b> {
    fn unwrap_send(self) -> &'b [u8] {
        match self {
            Output::Send(buf) => buf,
            _ => panic!("Expected Send"),
        }
    }

    fn unwrap_event(self) -> Event {
        match self {
            Output::Event(event) => event,
            _ => panic!("Expected Event got {:?}", self),
        }
    }
}

impl Event {
    fn unwrap_result(self) -> (u16, Option<Duration>) {
        match self {
            Event::Response { seq_num, rtt } => (seq_num, rtt),
        }
    }
}

#[derive(Default)]
pub struct BasicContext {
    buffer: Vec<u8>,
}

impl Context for BasicContext {
    fn buffer(&mut self, size: usize) -> &mut [u8] {
        self.buffer.resize(size, 0);
        &mut self.buffer
    }
}

// Turn into a min heap instead
impl Ord for Request {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.expires_at.cmp(&self.expires_at)
    }
}

impl PartialOrd for Request {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod test {
    use pnet::packet::icmp::echo_request::EchoRequestPacket;
    use pnet::packet::ip::IpNextHeaderProtocol;
    use pnet::packet::ipv4::MutableIpv4Packet;

    use super::*;

    struct TestContext {
        buffer: Vec<u8>,
    }

    impl Context for TestContext {
        fn buffer(&mut self, size: usize) -> &mut [u8] {
            self.buffer.resize(size, 0);
            &mut self.buffer
        }
    }

    #[test]
    fn test_starts_by_returning_echo() {
        let (mut ping, mut context, now) = setup();
        let input = Input::Time(now);
        let output = ping.handle_input(input, &mut context).unwrap();
        assert!(matches!(output, Output::Send(_)));
        let data = output.unwrap_send();
        validate_echo_request(&data);
    }

    #[test]
    fn test_handles_response() {
        let (mut ping, mut context, now) = setup();
        let input = Input::Time(now);
        let output = ping.handle_input(input, &mut context).unwrap();
        assert!(matches!(output, Output::Send(_)));
        let reply = make_echo_reply(0x1337, 0);

        let input = Input::Datagram(&reply, now + ms(23));
        let output = ping
            .handle_input(input, &mut context)
            .expect("should handle the response");
        let event = output.unwrap_event();
        let (seq_num, rtt) = event.unwrap_result();
        assert_eq!(seq_num, 0);
        assert_eq!(rtt, Some(ms(23)));
    }

    #[test]
    fn test_handle_response_timeout() {
        let (mut ping, mut context, now) = setup();
        let input = Input::Time(now);
        let output = ping.handle_input(input, &mut context).unwrap();
        assert!(matches!(output, Output::Send(_)));

        let input = Input::Time(now + ms(999));
        let output = ping.handle_input(input, &mut context).unwrap();
        assert!(
            matches!(output, Output::Timeout(_)),
            "No response or timeout after 999ms"
        );

        let input = Input::Time(now + ms(1000));
        let output = ping.handle_input(input, &mut context).unwrap();
        assert!(
            matches!(output, Output::Send(_)),
            "Should send another ping first"
        );

        let input = Input::Time(now + ms(1000));
        let output = ping.handle_input(input, &mut context).unwrap();
        let event = output.unwrap_event();
        let (seq_num, rtt) = event.unwrap_result();
        assert_eq!(seq_num, 0);
        assert_eq!(rtt, None, "Should have timed out");
    }

    fn setup() -> (Ping, impl Context, Instant) {
        let ping = Ping::new(Ipv4Addr::new(8, 8, 8, 8), Duration::from_secs(1));
        let context = TestContext { buffer: Vec::new() };
        let now = Instant::now();
        (ping, context, now)
    }

    fn validate_echo_request(buf: &[u8]) {
        let echo_request = EchoRequestPacket::new(buf).unwrap();
        assert_eq!(echo_request.get_icmp_type(), IcmpType(8));
        assert_eq!(echo_request.get_icmp_code(), IcmpCode(0));
        assert_eq!(echo_request.get_identifier(), 0x1337);
        assert_eq!(echo_request.get_sequence_number(), 0);
    }

    fn make_echo_reply(identifier: u16, seq_num: u16) -> Vec<u8> {
        let mut buf = vec![0_u8; 28];
        // IP header and ICMP echo reply
        {
            let mut packet = MutableIpv4Packet::new(&mut buf).expect("buffer is big enough");
            packet.set_version(4);
            packet.set_header_length(5);
            packet.set_dscp(0);
            packet.set_ecn(0);
            packet.set_total_length(20);
            packet.set_identification(0);
            packet.set_flags(0);
            packet.set_fragment_offset(0);
            packet.set_ttl(64);
            packet.set_next_level_protocol(IpNextHeaderProtocol(1));
            packet.set_source(Ipv4Addr::new(8, 8, 8, 8));
            packet.set_destination(Ipv4Addr::new(192, 168, 1, 37));
        }

        {
            let mut packet =
                MutableEchoRequestPacket::new(&mut buf[20..]).expect("buffer is big enough");
            packet.set_icmp_type(IcmpType(0));
            packet.set_icmp_code(IcmpCode(0));
            packet.set_identifier(identifier);
            packet.set_sequence_number(seq_num);
        }
        // Now we have a buffer with the ICMP packet, compute and set the checksum
        let mut icmp_packet = MutableIcmpPacket::new(&mut buf).expect("buffer is big enough");
        let checksum = checksum(&icmp_packet.to_immutable());
        icmp_packet.set_checksum(checksum);

        buf
    }

    fn ms(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }
}
