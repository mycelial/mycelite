## libsqlite-sys
Bindings to sqlite3, relies on system-installed sqlite3.  

## Doc
```bash
cargo doc --open
```

## Access to generated bindings
```bash
cargo expand
```  
or  
```bash
cat `find ../target/debug/build/ -name 'bindings.rs' -print`
```
