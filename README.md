![tests](https://github.com/mycelial/mycelite/actions/workflows/tests.yml/badge.svg)

# Mycelite

Mycelite implements physical single-master replication for SQLite. 

### Technical details

- Mycelite is a [VFS](https://www.sqlite.org/vfs.html) extension, which acts 
as a proxy for OS filesystem. 
- It intercepts page writes and creates a binary diff with the old version. 
- The binary diffs are then stored in the [journal](./journal/README.md). They can also be sent
over the network to another machine.
- The diffs in the journal can then be sequentially applied, thus achieving 
bit-perfect copy of the original database.

For more details on SQLite binary format see [sqlite-parser-nom](https://github.com/mycelial/sqlite-parser-nom).
In principle, it could be illustrated in the following way:

```
┌───────────┐ VFS write                     ┌────────────┐ apply ┌────────────────┐
│ db.sqlite ├──────────┐                    │ db.journal ├───────► replica.sqlite │
├───────────┤          │                    ├────────────┤       ├────────────────┤
│  header   │   ┌──────▼─────┐            ┌─►  diff 0    │       │    header      │
├───────────┤   │ page 0 new ├─┐          │ ├────────────┤       ├────────────────┤
│  page 0   ├─┐ └────────────┘ │ ┌──────┐ │ │    ...     │       │    page 0      │
├───────────┤ │                ├─► diff ├─┘ └────────────┘       ├────────────────┤
│  page 1   │ │ ┌────────────┐ │ └──────┘                        │      ...       │
└───────────┘ └─► page 0 old ├─┘                                 └────────────────┘
                └────────────┘
```

This approach comes with both significant upsides and downsides:
- Replica will contain exactly the same object in exactly the same order as in original.
- Out-of-the-box non-deterministic DDLs (e.g., UPDATE with RANDOM() or CURRENT_TIMESTAMP).
- Physical replication is less resource-intensive than logical replication, resulting in
higher throughput with no penalty as the number of replicas grows.
- Time travel by hydrating up to any previous timestamp.
- As there is no locking mechanism currently implemented, only a single writer is supported.
- Replica journal grows linearly, unless compacted.
- VACUUM operation might result in significantly sized journal entry without 
actual changes to accessible data.
- Currently, [WAL](https://www.sqlite.org/wal.html)-enabled databases are not supported.

### Usage
Refer to the [Quickstart Documentation](https://mycelial.com/docs/get-started/quick-start).

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
