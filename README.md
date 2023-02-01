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
You must be using Rust v1.65 or newer.

Install a recent version of SQLite. The default version of SQLite that comes
preinstalled may not support extensions. For Mac users, [Brew](https://formulae.brew.sh/formula/sqlite)
will install an appropriate build of SQLite.
_Note: you may need to modify your **$PATH**. Pay close attention to Brew's PATH instructions._

<!-- TODO: Do we want to have an instruction for devenv at this point? -->

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

<!-- TODO: are we switching to `owner` terminology? if so, we should consider introducing now -->

In a new terminal, start a SQLite writer instance with the following command:

<!-- TODO: should this be Mycelite or Mycelial Writer / Mycelial Reader? -->

```
cd mycelite
sqlite3
```

After SQLite's CLI opens, load the extension and open the database with the
following commands:

```
.load ./target/release/libmycelite
.open file:writer.db?vfs=mycelite_writer
```

#### SQLite Reader

In a new terminal, start a SQLite reader instance with the following command:

```bash
cd mycelite
sqlite3
```

After SQLite's CLI opens, load the extension and open the database with the
following commands:

```
.load ./target/release/libmycelite
.open reader.db
```

#### Configuration

Both the Mycelial Reader and Mycelial Writer require some initial configuration. Mycelial configuration is stored in a virtual table (vtable) called `mycelite_config`.

To configure your Mycelite, first load the vtable _in your SQLite shell_:

```
.load ./target/release/libmycelite mycelite_config
```

Next, insert values for the `endpoint` (replicator endpoint), the `domain` (see [Mycelial Domains]()) where the database lives), a `client_id`, and a `secret`.

<!-- TODO: page for domains, info for generating a client_id and secret?? -->

```
insert into mycelite_config values
    ('endpoint', 'http://localhost:8080'),
    ('domain', 'domain'),
    ('client_id', 'client_id'),
    ('secret', 'secret');
```

Validate config:

```
select * from mycelite_config;
+-----------+-----------------------+
|    key    |         value         |
+-----------+-----------------------+
| client_id | client_id             |
| domain    | domain                |
| endpoint  | http://localhost:8080 |
| secret    | secret                |
+-----------+-----------------------+
```

Configuration is persistent and written to a file at `\<database-filename\>-mycelite-config.

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
