# The Mycelite Journal

The Mycelite Journal provides the ability to capture SQLite page changes at the transaction level.

![Mycelial VFS Journal V1](https://user-images.githubusercontent.com/504968/204807386-5da165b7-6aef-44ca-ac09-b736c666c297.png)

The Mycelite Journal is composed of two components: the Header, which begins the Journal, and one or more snapshots.

## Mycelite Journal Header

The first 128 bytes of the Mycelite Journal is the journal header. These 128 bytes contain:

- Magic (4 bytes) - Constant value of `0x00907a70` (read as "potato"). Why potato? Because the engineer who built this decided to call it potato. (For more information on file signatures known as "Magic Bytes," see [List of File Signatures](https://en.wikipedia.org/wiki/List_of_file_signatures)).

- Version (4 bytes) - Represents the Journal version number.

- Snapshot Counter (8 bytes) - The number of Snapshots in the Journal.

- EOF (8 bytes)- This is the offset position of the commited Snapshot, and designates the end of the file.

- The balance of the Journal's header bytes are reserved space.

## Mycelite Journal Snapshots

Each Snapshot represents a SQLite transaction that has been captured by the Mycelite Journal.

Snapshots are comprised of two components: one Snapshot Header and one or more SQLite pages.

### Snapshot Header

The first 32 bytes of a Mycelite Journal Snapshot is the Snopshot Header. These 32 bytes contain:

- Id (8 bytes) - a unique id(?) for this Journal Snapshot
- Timestamp (8 bytes) - UTC unixtime timestamp in microseconds
- Reserved (16 bytes) - reserved space

### Page Header

Each SQLite page _also_ has a header, called a Page Header.

The first 16 bytes of a SQLite page is the Page Header. These 16 bytes contain:

- Offset (8 bytes) - The offset in the database at which this SQLite page was written
- Page Num - Each SQLite database page has its own number. The first page of a Snapshot starts with 0, and the value is incremented as pages are added.
- Page Size - Represents the Journal page size, which is currently set to the underlying SQLite page size.

### Snapshot EOF (end of file)

The Snapshot ends with the last Page Header, whose values are all set to `0`.
