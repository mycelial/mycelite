![tests](https://github.com/mycelial/mycelite/actions/workflows/tests.yml/badge.svg)
# Mycelite

## What is Mycelite?

Mycelite is a SQLite extension that allows you to synchronize changes from one
instance of SQLite to another. Currently, it only supports one-way
synchronization, but eventually, it will support two-way synchronization.

Why would you want to synchronize multiple SQLite databases? Read on to learn.

### A new type of application

There is a new type of application called local-first, which combines many of
the best features from both local and client/server applications.

### What does local-first offer?

With local-first applications, you get the speed and responsiveness of a local
application, but at the same time, you get many of the desirable features from
client/server systems.

### What do local-first applications look like?

A good example of a local-first application is [Actual
Budget](https://github.com/actualbudget/actual), an open-source personal finance
application.

What makes Actual Budget different from its competitors?

First of all, it's very fast because all the application data is on the local
device - in a SQLite database - but what's most interesting about this app is
that it works on multiple devices. In other words, it has apps for iOS, Android,
Windows, Mac and the Web and it allows you to make concurrent changes on
multiple devices and synchronize those changes to all your devices.

### Why aren't more developers creating local-first applications?

Actual Budget is a good example of a local-first application, but it wasn't very
easy to build. The authors had to write a bunch of synchronization-related code,
that implements and uses
[CRDTs](https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type), and
this start-from-scratch approach just isn't practical for most situations.
Building local-first applications today is too difficult, but we're going to
change that.

## Quickstart

### Prerequisites

[Install Rust](https://www.rust-lang.org/tools/install).
The version of Rust must be >= 1.65.

Install a recent version of SQLite. The default version of SQLite that comes
preinstalled may not support extensions. For Mac users, [Brew](https://formulae.brew.sh/formula/sqlite)
will install an appropriate build of SQLite.
_Note: you may need to modify your **$PATH**. Pay close attention to Brew's PATH instructions._

### Building Mycelite

After cloning the repo, run the following to build Mycelite:

```bash
git clone git@github.com:mycelial/mycelite.git
cd mycelite
cargo build --release
```

This will create a shared library in `./target/release/libmycelite.dylib`.

### Command Line (CLI)

#### Replicator

Start the replicator service with the following terminal command:

```bash
cd mycelite/examples
cargo run -p sync-backend
```

#### SQLite Writer

In a new terminal, start a SQLite writer instance with the following command:

```
cd mycelite
MYCELIAL_WRITER=true sqlite3
```

After SQLite's CLI opens, load the extension and open the database with the
following commands:

```
.load ./target/release/libmycelite
.open writer.db
```

#### SQLite Reader

In a new terminal, start a SQLite reader instance with the following command:

```bash
cd mycelite
MYCELIAL_WRITER=false sqlite3
```

After SQLite's CLI opens, load the extension and open the database with the
following commands:

```
.load ./target/release/libmycelite
.open reader.db
```

#### Observing Synchronization

In the writer instance, create a table and then populate the table. For example:

```sql
create table test(int);
insert into test values (42);
```

Now in the reader instance, you will see the new table and you can query its
values. For example:

```sql
.tables
select * from test; -- # returns 42
```

## License

Apache 2.0
