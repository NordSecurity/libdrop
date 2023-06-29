### v5.0.0
### **Broken Records**
---
* Persist transfers across libdrop shutdowns. Automatically restart the transfer as soon as the connection can be established.

---
<br>

### v4.1.0
### **UNRELEASED**
---
* Add IPv6 support
* Add file rejections via `norddrop_reject_file()`
* Switch to rusqlite for persistence

---
<br>

### v4.0.0
### **Relentless Records**
---
* Persist transfers to SQLite

---
<br>

### v3.1.1
### **ANGERY^3**
---
* Fix the go bindings being unusable, again

---
<br>

### v3.1.0
### **ANGERY^2**
---
* Remove reporting already downloaded file as finished. Instead, redownload it with (1) suffix 
* Fix the go bindings being unusable
* Fix directory flattening on Windows
* Use `byte[]` for the private key in C# bindings

---
<br>

### v3.0.0
### **ANGERY**
---
* Accept `storage_path` through `norddrop_start` config for SQLite persistence
* File IDs are no longer valid paths. Changed the structure of `RequestQueued/Received` events to contain a flat file list
* Fix duplicate filenames error
* Removed libnorddrop 2.0.0 wire protocol, because of the security issue

---
<br>

### v2.0.0
### **Brzęczyszczykiewicz**
---
* Fix not receiving `FileUploadStarted` and `FileUploadSuccess` events before the transfer is canceled
* Fix the wrong `by_peer` field in `FileCanceled` event when the sender cancels the file upload
* Return moose init error only on non-prod
* When requesting the file, perform the check if the file is already downloaded. If yes, report successful file transfer
* Do not remove temporary files in case of transfer failure, resume transfer when the temporaries are available
* Return more status codes from the ffi layer methods
* The `FileDownloaded` event reports the full file path
* Improve moose error handling
* Configure Android SONAME
* Introduce authentication with X25519 keys, accept private key and peer public key callback in `norddrop_new()`

---
<br>

### v1.1.1
### **More Data Filled Dumplings**
---
* Add windows dll version info
* Emit `Progress` event only when at least 64K of file data received since the last report

---
<br>

### v1.1.0
### **More Data Filled Dumplings**
---
* Replace illegal characters and file names in the download location path with the valid ones
* Issue ping/pong over the WS connection periodically 
* Add `connection_max_retry_interval_ms` config parameter for controlling the maximal interval between connection retries. Default(10,000)
* Monitor files during transfers for modification
* Rename the directory if its name conflicts with the existing one 

---
<br>

### v1.0.0
### **Data Filled Dumplings**
---
* Implement moose v0.1.2

<br>
