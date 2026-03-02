# Channels

Channels provide thread-safe message passing between concurrent tasks. A channel is a bounded, synchronous queue that allows one task to send values and another to receive them.

## Creating a Channel

### Syntax

```
channel()
channel(capacity)
```

The `channel` builtin creates a new channel and returns a `Channel` value. An optional integer argument specifies the buffer capacity (default: 32).

```forge
let ch = channel()        // buffered channel, capacity 32
let ch = channel(100)     // buffered channel, capacity 100
let ch = channel(1)       // minimal buffer, capacity 1
```

If a non-integer argument is provided, the default capacity of 32 is used. The minimum capacity is 1 (values less than 1 are clamped to 1).

## Channel Value

A channel is represented at runtime as `Value::Channel(Arc<ChannelInner>)`. The `ChannelInner` struct contains:

- `tx: Mutex<Option<SyncSender<Value>>>` — the sender half.
- `rx: Mutex<Option<Receiver<Value>>>` — the receiver half.
- `capacity: usize` — the buffer capacity.

Channels are reference-counted via `Arc`, so they can be safely shared between the parent task and spawned tasks through environment cloning.

## Sending Values

### send(channel, value)

The `send` builtin sends a value through a channel. It blocks if the channel buffer is full, waiting until a receiver consumes a value.

```forge
let ch = channel()
send(ch, 42)
send(ch, "hello")
send(ch, [1, 2, 3])
```

Any Forge value can be sent through a channel: integers, strings, arrays, objects, functions, Results, and other channels.

**Arguments:**

- `channel` — A `Channel` value (first argument).
- `value` — Any `Value` to send (second argument).

**Returns:** `null` on success.

**Errors:**

- `"send(channel, value) requires 2 arguments"` — Wrong argument count.
- `"send() requires a channel as first argument"` — First argument is not a channel.
- `"channel closed"` — The receiver has been dropped.

### try_send(channel, value)

The `try_send` builtin attempts to send a value without blocking. Returns `true` if the value was sent, `false` if the channel is full or closed.

```forge
let ch = channel(1)
send(ch, "first")            // fills the buffer
let ok = try_send(ch, "second")  // false — buffer is full
```

**Arguments:** Same as `send`.

**Returns:** `Bool` — `true` if sent, `false` otherwise.

**Errors:**

- `"try_send() requires (channel, value)"` — Wrong argument count.
- `"try_send() first argument must be a channel"` — First argument is not a channel.

## Receiving Values

### receive(channel)

The `receive` builtin receives a value from a channel. It blocks until a value is available.

```forge
let ch = channel()
send(ch, 42)
let val = receive(ch)    // 42
```

If the channel is closed (all senders dropped) and the buffer is empty, `receive` returns `null`.

**Arguments:**

- `channel` — A `Channel` value.

**Returns:** The received `Value`, or `null` if the channel is closed.

**Errors:**

- `"receive(channel) requires 1 argument"` — No argument provided.
- `"receive() requires a channel as first argument"` — Argument is not a channel.

### try_receive(channel)

The `try_receive` builtin attempts to receive a value without blocking. Returns `Some(value)` if a value was available, `None` if the channel is empty.

```forge
let ch = channel()
let result = try_receive(ch)    // None — nothing sent yet

send(ch, 42)
let result = try_receive(ch)    // Some(42)
```

**Arguments:**

- `channel` — A `Channel` value.

**Returns:** `Some(value)` if a value was received, `None` if the channel is empty or closed.

**Errors:**

- `"try_receive() requires a channel"` — No argument provided.
- `"try_receive() argument must be a channel"` — Argument is not a channel.

## Producer-Consumer Pattern

Channels enable classic producer-consumer patterns:

```forge
let ch = channel()

// Producer
spawn {
    repeat 5 times {
        send(ch, it)
    }
}

// Consumer
repeat 5 times {
    let val = receive(ch)
    say "got: " + str(val)
}
```

## Fan-Out Pattern

Multiple consumers can share a channel, though only one will receive each message:

```forge
let work = channel()
let results = channel()

// Producer
spawn {
    for item in tasks {
        send(work, item)
    }
}

// Workers
repeat 3 times {
    spawn {
        let item = receive(work)
        let result = process(item)
        send(results, result)
    }
}
```

## Channel Lifetime

Channels remain open as long as at least one reference exists. When all references to a channel are dropped (through garbage collection or scope exit), the underlying `SyncSender` and `Receiver` are dropped, which closes the channel. Subsequent `send` calls on a closed channel return an error; `receive` calls return `null`.
