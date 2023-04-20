### UNRELEASED
### **ANGERY**
---
* Accept `storage_path` through `norddrop_start` config for SQLite persistence

---
<br>

### v2.0.0
### **Brzęczyszczykiewicz**
---
* When requesting the file, perform the check if the file is already downloaded. If yes, report successful file transfer
* Do not remove temporary files in case of transfer failure, resume transfer when the temporaries are available
* Return more status codes from the ffi layer methods
* The `FileDownloaded` event reports the full file path
* Improve moose error handling
* Configure Android SONAME
* Introduce authentication with ed25519 keys, accept private key and peer public key callback in `norddrop_new()`

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
