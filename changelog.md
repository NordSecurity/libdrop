### v5.5.0-moose-with-context
### **Unforeseen Moose**
---
* Update moose tracker to 6.0.1

---
<br>

### v5.4.0
### **Unforeseen Moose**
---
* Update moose tracker to v2.0.0
* Add `moose_app_version` field to config
* Add timestamps to emitted events
* Add DLL signing to Windows builds

---
<br>

### v5.3.0
### **Tungsten Bullet**
---
* Fix the problem where sometimes the `FileDownloaded` event was not emitted when the peers were disconnecting rapidly
* Ensure outstanding file streaming tasks are finished before removing temporary files. Add more logging around temporary files handling
* Try to remove temporary files right away in the case of rejection 
* Update the rustc to 1.72.1

---
<br>

### v5.2.0
### **Golden Bullet**
---
* Use unbounded events queue in order to disable back pressure on the connection handlers in case the event callback blocks.
* Improve the suppression of the `TransferRequest` and `TransferCancelled` event pair for cancelled transfers.
* Fix same named directories being merged on the sender side
* Disallow transfers with no files in them

---
<br>

### v5.1.0
### **Silver Bullet**
---
* Suppress synchronization of transfers cancelled before the receiver got a chance to receive the transfer request.\
This fixes the case when transfer cancelled on the sender side caused `TransferCancelled` being fired immedieatly after `TransferRequest`.\
With this change the transfer would not be seen on the receiver side at all.
* Implement a periodic transfer state check in case of stalled transfer for the receiver
* Fix occasional events reordering, for example file done swapped transfer cancelation, on the sender side 
* Fix repeated `FileDone` events on the sender, when reconnecting to the  receiver.

---
<br>

### v5.0.1
### **Broken Records**
---
* Mitigate trasnfer cancelation being out of sync on high latency network
* Periodic moose context updates(600s)

---
<br>

### v5.0.0
### **Broken Records**
---
* Persist transfers across libdrop shutdowns. Automatically restart the transfer as soon as the connection can be established.
* Add `max_uploads_in_flight` config option to limit the number of files being concurrently uploaded
* Add `norddrop_set_fd_resolver_callback()` function for providing the content URI callback for Android
* Add `max_requests_per_sec` config option to limit the requests on per-peer basis
* Add file descriptors resolver callback based on content URI - Unix platform only
* Fix database foreign keys not being enabled
* Remove `ServiceStop`, `UnexpectedData`, `TransferTimeout`, `WsServer`, `WsClient` status codes
* Add `FileRejected`, `FileFailed` and `FileFinished` status codes
* Remove `cancel_file()` method in favor of rejections
* Introduce `TransferPaused` event signaling file download being paused because of peer disconnection
* Allow removing file only upon reaching one Rejected, Failed or Finished state
* Disallow re-downloading file after reaching Rejected, Failed or Finished state within single transfer
* Include `bytes_sent/bytes_received` JSON field per file in the storage output
* Remove `transfer_idle_lifetime_ms`, `connection_max_retry_interval_ms`, `max_uploads_in_flight` and `max_requests_per_sec` config parameters
* Remove transfer `active` state in JSON api

---
<br>

### v4.2.0
### **Little rejection**
---
* Fix rejection states not being present in the storage JSON output
* Add `norddrop_remove_transfer_file()` method for removing rejected files from the database

---
<br>

### v4.1.0
### **SQL: Save, Query, Love**
---
* Add IPv6 support
* Add file rejections via `norddrop_reject_file()`
* Switch to rusqlite for persistence
* Try to open database in memory if path fails
* Report database opening errors to moose
* Use in memory database if on-disk fails
* Checksum validation at the end of download
* More trace logs on persistence layer

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
