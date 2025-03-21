---
theme:
#  path: theme.yaml
---


<!-- jump_to_middle -->
```rust
// There will be code, here's how large the font will be.
fn main() {
    println!("Hello, world!");
}
```

<!-- end_slide -->

# Me

_Hugo of all trades_

<!-- jump_to_middle -->

* Originally from Sweden, called Edinburgh home for almost 10 years
* Rust since 2017, professionally at Lookback since 2021

<!-- end_slide -->

# Background - Coloured functions

"What color is your function?" by Bob Nystrom

<!-- pause -->

<!-- incremental_lists: true -->
* function <span style="color: palette:blue">blue</span>() {}
* async function <span style="color: palette:red">red</span>() {}

<!-- end_slide -->

# Background - Coloured functions


<!-- column_layout: [3, 2] -->

<!-- column: 0 -->

```javascript
async function red() {
    const blueResult = blue();
    return await otherRed(blueResult);
}
```

<!-- pause -->
<!-- column: 1 -->

We can call blue functions from red functions.

<!-- pause -->
<!-- column: 0 -->

```javascript
function blue() {
    // Missing `await` 
    const redResult = red();
    return otherBlue(redResult);
}
```

<!-- pause -->
<!-- column: 1 -->
<!-- newlines: 5 -->

We cannot call red functions from blue functions.

<!-- pause -->
<!-- column: 0 -->

```javascript
async function noLongerBlue() {
    const redResult = await red();
    return otherBlue(redResult);
}
```

<!-- pause -->
<!-- column: 1 -->
<!-- newlines: 6 -->

Must convert `blue` to a red function to call `red`. Red functions spread like wildfire.

<!-- end_slide -->

<!-- jump_to_middle -->

Enough Javascript, let's talk Rust
=

<!-- end_slide -->

# Background - Coloured functions

"Let futures be futures" by withoutboats

<!-- pause -->
<!-- incremental_lists: true -->
* fn <span style="color: palette:blue">blue</span>() {}
* async fn <span style="color: palette:red">red</span>() {}
* fn <span style="color: palette:light_green">green</span>() {}

<!-- end_slide -->

# Background - Coloured functions

<!-- jump_to_middle -->

<!-- font_size: 2 -->

| Caller  / Callee ⟶   | <span style="color: palette:blue">Blue</span> | <span style="color: palette:red">Red</span> | <span style="color: palette:light_green">Green</span>|
|----------|---------------|--------------|----------------|
| <span style="color: palette:light_green">Green</span>    | **No**        | **No**       | Yes            |
| <span style="color: palette:blue">Blue</span>     | Yes           | **No**       | Yes            |
| <span style="color: palette:red">Red</span>      | **No**        | Yes          | Yes            |


<!-- end_slide -->

# Background - Coloured functions

I lied

<!-- incremental_lists: true -->

* async fn <span style="color: palette:red">red</span>() {}
* async fn <span style="color: palette:aqua">teal</span>() {}
* async fn <span style="color: palette:orange">orange</span>() {}
* async fn <span style="color: palette:purple">purple</span>() {}

<!-- end_slide -->
# Background - Coloured functions

* async fn <span style="color: palette:red">read</span>(stream: &tokio::net::TcpStream) {}
* async fn <span style="color: palette:aqua">read</span>(stream: &glommio::net::TcpStream) {}
* async fn <span style="color: palette:orange">read</span>(stream: &async_net::TcpStream) {}
* async fn <span style="color: palette:purple">read</span>(stream: &tokio_uring::net::TcpStream) {}

<!-- end_slide -->

<!-- jump_to_middle -->

Sans-IO
=
<!-- end_slide -->

# Sans-IO - What is it?

<!-- pause -->

<!-- incremental_lists: true -->

* Originally from the Python world.
* Make all functions <span style="color: palette:light_green">green</span>.
* Leverage inversion of control.
* Not just for IO.
* ping(8)


<!-- end_slide -->
# Sans-IO - How

<!-- pause -->

```rust
struct Ping {
    // Attributes omitted
}

impl Ping {
    fn handle_input(&mut self, input: Input) -> Output {
        todo!()
    }
}
```
<!-- end_slide -->


# Sans-IO - How


```rust +line_numbers {1-6|2-3|4-6|8-15|9-10|11-12|13-22}
enum Input<'b> {
    /// Some input data and the current time.
    Data((&'b [u8], Instant)),
    /// Timeout reached, here's the current time.
    Time(Instant),
}

enum Output {
    /// Send this data to the network.
    Send(Vec<u8>),
    /// Ask me again at this time.
    Timeout(Instant),
    /// An event occurred.
    Event(Event),
}

enum Event {
    Result {
        seq_num: u16,
        rtt: Option<Duration>,
    }
}
```
<!-- end_slide -->

# Sans-IO - How


```rust +line_numbers {1-8|4|5}
impl Ping {
    fn handle_input(&mut self, input: Input) -> Result<Output, Error> {
        match input {
            Input::Datagram(buf, now) => self.hande_datagram(buf, now),
            Input::Time(now) => Ok(self.handle_timeout(now)),
        }
    }
}
```
<!-- end_slide -->

# Sans-IO - How


```rust +line_numbers {1-22|2-10|13-17|18-19|12-20}
fn main() -> Result<()> {
    let socket = create_socket()?;
    let addr: SocketAddrV4 = "8.8.8.8:0".parse()?;
    let sock_addr = SockAddr::from(addr);
    // Construct the sans-IO core struct, Ping.
    let mut ping = Ping::new(*addr.ip(), Duration::from_millis(1000));

    let mut buf: [MaybeUninit<u8>; 1500] = [MaybeUninit::uninit(); 1500];
    let mut timeout_until = Instant::now();
    let mut last_recv: Option<Input<'_>>;

    loop {
        // 1. Read from the socket or timeout
        let timeout = (timeout_until - Instant::now()).max(Duration::from_millis(1));
        socket.set_read_timeout(Some(timeout))?;
        last_recv = read_from_socket(&socket, &mut buf)?;

        // 2. Handle the input
        timeout_until = handle_input(&mut last_recv, &mut ping, &socket, addr, &sock_addr)?;
    }
}
```
<!-- end_slide -->

# Sans-IO - How


```rust +line_numbers {1-19|9|13|15}
fn read_from_socket<'b>(
    socket: &Socket,
    buf: &'b mut [MaybeUninit<u8>; 1500],
) -> Result<Option<Input<'b>>> {
    match socket.recv_from(buf) {
        Ok((len, _)) => {
            // SAFETY: We read at least `len` bytes into `buf`, so it's safe to read from it.
            let read = unsafe { slice_assume_init_ref(&buf[..len]) };
            Ok(Some(Input::Datagram(read, Instant::now())))
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                Ok(Some(Input::Time(Instant::now())))
            } else {
                return Err(e.into());
            }
        }
    }
}
```
<!-- end_slide -->

# Sans-IO - How


```rust +line_numbers {1-26|9-12|15-17|18-20|21-23}
fn handle_input(
    last_recv: &mut Option<Input<'_>>,
    ping: &mut Ping,
    socket: &Socket,
    addr: SocketAddrV4,
    sock_addr: &SockAddr,
) -> Result<Instant> {
    loop {
        let input = last_recv
            .take()
            .unwrap_or_else(|| Input::Time(Instant::now()));
        let output = ping.handle_input(input)?;

        match output {
            ping::Output::Event(event) => {
                handle_event(event, addr);
            }
            ping::Output::Send(vec) => {
                socket.send_to(&vec, sock_addr)?;
            }
            ping::Output::Timeout(instant) => {
                break Ok(instant);
            }
        }
    }
}
```
<!-- end_slide -->

# Sans-IO - How


```rust +line_numbers {12-20}
fn main() -> Result<()> {
    let socket = create_socket()?;
    let addr: SocketAddrV4 = "8.8.8.8:0".parse()?;
    let sock_addr = SockAddr::from(addr);
    // Construct the sans-IO core struct, Ping.
    let mut ping = Ping::new(*addr.ip(), Duration::from_millis(1000));

    let mut buf: [MaybeUninit<u8>; 1500] = [MaybeUninit::uninit(); 1500];
    let mut timeout_until = Instant::now();
    let mut last_recv: Option<Input<'_>>;

    loop {
        // 1. Read from the socket or timeout
        let timeout = (timeout_until - Instant::now()).max(Duration::from_millis(1));
        socket.set_read_timeout(Some(timeout))?;
        last_recv = read_from_socket(&socket, &mut buf)?;

        // 2. Handle the input
        timeout_until = handle_input(&mut last_recv, &mut ping, &socket, addr, &sock_addr)?;
    }
}
```
<!-- end_slide -->

# Sans-IO - How

<!-- column_layout: [2, 2] -->

<!-- column: 0 -->

```rust +line_numbers
fn main() -> Result<()> {
    let socket = create_socket()?;
    let addr: SocketAddrV4 = "8.8.8.8:0".parse()?;
    let sock_addr = SockAddr::from(addr);
    // Construct the sans-IO core struct, Ping.
    let mut ping = Ping::new(*addr.ip(), Duration::from_millis(1000));

    let mut buf: [MaybeUninit<u8>; 1500] = [MaybeUninit::uninit(); 1500];
    let mut timeout_until = Instant::now();
    let mut last_recv: Option<Input<'_>>;

    loop {
        // 1. Read from the socket or timeout
        let timeout = (timeout_until - Instant::now()).max(Duration::from_millis(1));
        socket.set_read_timeout(Some(timeout))?;
        last_recv = read_from_socket(&socket, &mut buf)?;

        // 2. Handle the input
        timeout_until = handle_input(&mut last_recv, &mut ping, &socket, addr, &sock_addr)?;
    }
}
```

<!-- column: 1 -->

```rust +line_numbers
#[tokio::main]
async fn main() -> Result<()> {
    let addr: SocketAddrV4 = "8.8.8.8:0".parse()?;
    let mut socket = Socket::new_icmp_v4(addr)?;
    // Construct the sans-IO core struct, Ping.
    let mut ping = Ping::new(*addr.ip(), Duration::from_millis(1000));

    let mut buf = [0u8; 1500];
    let mut timeout_until = Instant::now();
    let mut last_recv: Option<Input<'_>>;

    loop {
        // 1. Read from the socket or timeout
        let timeout = (timeout_until - Instant::now()).max(Duration::from_millis(1));
        last_recv = Some(read_from_socket(&mut socket, &mut buf, timeout).await?);

        // 2. Handle the input
        timeout_until = handle_input(&mut last_recv, &mut ping, &mut socket, addr).await?;
    }
}
```

<!-- end_slide -->

# Sans-IO - Testing


```rust +line_numbers 
#[test]
fn test_starts_by_returning_echo() {
    let (mut ping, now) = setup();
    let input = Input::Time(now);
    let output = ping.handle_input(input).unwrap();
    assert!(matches!(output, Output::Send(_)));
    let data = output.unwrap_send();
    validate_echo_request(&data);
}
```

<!-- end_slide -->

# Sans-IO - Testing


```rust +line_numbers {1-17|9,16}
#[test]
fn test_handles_response() {
    let (mut ping, now) = setup();
    let input = Input::Time(now);
    let output = ping.handle_input(input).unwrap();
    assert!(matches!(output, Output::Send(_)));
    let reply = make_echo_reply(0x1337, 0);

    let input = Input::Datagram(&reply, now + ms(23));
    let output = ping
        .handle_input(input)
        .expect("should handle the response");
    let event = output.unwrap_event();
    let (seq_num, rtt) = event.unwrap_result();
    assert_eq!(seq_num, 0);
    assert_eq!(rtt, Some(ms(23)));
}
```

<!-- end_slide -->

# Sans-IO - Testing


```rust +line_numbers {1-28|8-13|15-27}
#[test]
fn test_handle_response_timeout() {
    let (mut ping, now) = setup();
    let input = Input::Time(now);
    let output = ping.handle_input(input).unwrap();
    assert!(matches!(output, Output::Send(_)));

    let input = Input::Time(now + ms(999));
    let output = ping.handle_input(input).unwrap();
    assert!(
        matches!(output, Output::Timeout(_)),
        "No response or timeout after 999ms"
    );

    let input = Input::Time(now + ms(1000));
    let output = ping.handle_input(input).unwrap();
    assert!(
        matches!(output, Output::Send(_)),
        "Should send another ping first"
    );

    let input = Input::Time(now + ms(1000));
    let output = ping.handle_input(input).unwrap();
    let event = output.unwrap_event();
    let (seq_num, rtt) = event.unwrap_result();
    assert_eq!(seq_num, 0);
    assert_eq!(rtt, None, "Should have timed out");
}
```

<!-- end_slide -->

# Sans-IO - Inversion of allocation


```rust +line_numbers
#[derive(Debug)]
pub enum Output<'b> {
    /// An event that happened
    Event(Event),
    /// Some data to send
    Send(&'b [u8]),
    /// We don't need to do anything until more data is received or the timeout is reached
    Timeout(Instant),
}

pub trait Context {
    fn buffer(&mut self, size: usize) -> &mut [u8];
}

pub fn handle_input<'s: 'b, 'b>(
    &'s mut self,
    input: Input,
    context: &'b mut impl Context,
) -> Result<Output<'b>, Error> {
    // omitted
}

```
<!-- end_slide -->

<!-- jump_to_middle -->

I need you to make sans-IO crates
=

<!-- end_slide -->

# Cool stuff

## sans-io crates

* **quinn**- QUIC/HTTP3 implementation https://github.com/quinn-rs/quinn
* **str0m** - WebRTC implementation https://github.com/algesten/str0m/
* **librice** - ICE implementation https://github.com/ystreet/librice
* **rc-zip** - ZIP file handling https://github.com/bearcove/rc-zip

## Related interesting stuff

* **Coroutines** - To build sans-IO state machines.
* **Abusing Futures** - To build sans-IO state machines with async/await.
* **Effects** - Powerful abstractions for being generic over sync/async among other things.
* **Keyword generics initiative** Upcoming proposal to allow being generic of sync/async in Rust.
