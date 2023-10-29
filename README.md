# AvSocket

A crate to help with [Unix Domain Sockets](https://en.wikipedia.org/wiki/Unix_domain_socket) the `async` way.

## Examples

### Shared Protocol

1. Make a shared library crate (here called `proto`).
  1. Add your custom `struct`s and `enum`s, and be sure to add `#[derive(serde::Serialize, serde::Deserialize)]` to make them work with `serde`.
  2. Add your custom method definitions with the `declare!` macro.
  3. Your `proto` crate should look something like this:
```rust
use std::fmt::Debug;
use serde::{Serialize, Deserialize};
use avsocket::declare;

#[derive(Debug, Serialize, Deserialize)]
pub struct Goblin {
  pub health: i32,
  pub hungry: bool,
}

declare!(extern fn hurt(Goblin, i32) -> Goblin);
```

2. Make a `server` crate, and add your `proto` crate as a dependency.
  1. Implement your protocol's methods.
  2. Write the `main` function.
  3. Your `server` crate should look something like:
```rust
use proto::*;
use avsocket::server::{Handler, Server};

// NOTE [*1]: This signature must exactly match your `proto` crate's declaration.
fn hurt_impl(mut enemy: Goblin, damage: i32) -> Goblin {
  enemy.health -= damage;
  enemy
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let path = /* Some path preferably in `/run/user/{uid}/...`, or another well-known path. */;

  let mut handler = Handler::default();
  handler
    .add(hurt, &hurt_impl);   // <-- Make sure to add the borrow here (for now).
    // You can also use inline closures:
    /* .add(hurt, &|mut enemy, damage| {
          enemy.health -= damage;
          enemy
        })
    */

  Server::run(&path, handler).await
}
```

3. Make a `client` crate, and add your `proto` crate as a dependency (again).
  1. Write your `main` function to look like this:
```rust
use proto::*;
use avsocket::client::Dispatcher;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let path = /* Yada, yada, some well-known path to the client and the server... */;

  // The `Dispatcher` struct abstracts away the communication to make the
  //    socket communication look like a normal async function call.
  let mut dispatcher = Dispatcher::connect(&path).await?;

  let steve = Goblin {
    health: 20,
    hungry: true, // You should eat, Steve...
  };

  println!("Steve before: {steve:?}");

  // Here, the dispatcher wraps what looks like a function call to `proto::hurt`.
  // The dispatcher will actually do the dirty work and return a `Future<Output = Goblin>`.
  let steve = dispatcher.dispatch(hurt(steve, 23)).await;

  println!("Steve after: {steve:?}");

  Ok(())
}
```
