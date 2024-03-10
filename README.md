# Game backend template
Websocket/TCP game backend powered by Actix / Rust.
Designed around Room based matching. Targetted primarily towards casual / party based multiplayer games.

### Protocol
Currently designed to communicate with JSON over websockets and TCP (WIP), although binary would be far more preferrable.
Messages are divided primarily into two categories,

```rust
#[derive(serde::Deserialize)]
// Messages coming from the client to the server
enum IncomingMessage {
  ...variants
}
```

and

```rust
#[derive(serde::Serialize)]
// Messages going from server to client
enum OutgoingMessage {
  ...variants
}
```

Most control operations requested by the client such as joining rooms, starting games, etc involve an incoming request message for which
the server responds with a result with the appropriate status. `Result`s should contain a tag pointing to the associated operation (see type definition
for more information).
