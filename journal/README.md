## Journal (V1)
Journal provides ability to capture sqlite page changes on transaction level.  
Journal starts with header and proceeds with snapshots.

## Header
first 128 bytes of journal, which contain:
* magic (4 bytes) - constant value of `0x00907a70` (read as potato)
* version (4 bytes) - currently always 1
* snapshot counter (8 bytes) - each snapshot has unique id, on snapshot initialization current value stored as a snapshot id, journal header value increments
* eof (8 bytes)- offset to last commited snapshot
* reserved space


## Snapshot
Snapshot is a captured sqlite transaction.  
Snapshot consists of snapshot header and sqlite pages.  
Each sqlite page has own page header.  
Snapshot ends with 'last' page header, where page header values are all 0  


## Snapshot Header
snapshot header is a structure of 32 bytes:
* num (8 bytes) - unique for this journal number
* timestamp (8 bytes) - UTC unixtime timestamp in micros
* reserved (16) - reserved space (in case if we want to implement journal truncation, 16 bytes should be enough to store information about acked on backend snapshot)


## Page Header
page header is a 16 byte structure:
* offset (8 bytes) - offset in database at which sqlite page was written
* page num - each page has own number, first page is snapshot starts with 0, value is incremented as pages are added
* page_size - page size, currently equals to sqlite page size, but with diffing it will not be the case.


![Mycelial VFS Journal V1](https://user-images.githubusercontent.com/504968/204807386-5da165b7-6aef-44ca-ac09-b736c666c297.png)
