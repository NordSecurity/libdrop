#ifndef NORDDROP_H
#define NORDDROP_H

/* Generated with cbindgen:0.24.3 */

/* Warning, this file is autogenerated by cbindgen. Don't modify this manually. */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Posible log levels.
 */
typedef enum norddrop_log_level {
  NORDDROP_LOG_CRITICAL = 1,
  NORDDROP_LOG_ERROR = 2,
  NORDDROP_LOG_WARNING = 3,
  NORDDROP_LOG_INFO = 4,
  NORDDROP_LOG_DEBUG = 5,
  NORDDROP_LOG_TRACE = 6,
} norddrop_log_level;

typedef enum norddrop_result {
  /**
   * Operation was success
   */
  NORDDROP_RES_OK = 0,
  /**
   * Operation resulted to unknown error.
   */
  NORDDROP_RES_ERROR = 1,
  /**
   * Failed to parse C string, meaning the string provided is not valid UTF8
   * or is a null pointer
   */
  NORDDROP_RES_INVALID_STRING = 2,
  /**
   * One of the arguments provided is invalid
   */
  NORDDROP_RES_BAD_INPUT = 3,
  /**
   * Failed to parse JSON argument
   */
  NORDDROP_RES_JSON_PARSE = 4,
  /**
   * Failed to create transfer based on arguments provided
   */
  NORDDROP_RES_TRANSFER_CREATE = 5,
  /**
   * The libdrop instance is not started yet
   */
  NORDDROP_RES_NOT_STARTED = 6,
  /**
   * Address already in use
   */
  NORDDROP_RES_ADDR_IN_USE = 7,
  /**
   * Failed to start the libdrop instance
   */
  NORDDROP_RES_INSTANCE_START = 8,
  /**
   * Failed to stop the libdrop instance
   */
  NORDDROP_RES_INSTANCE_STOP = 9,
  /**
   * Invalid private key provided
   */
  NORDDROP_RES_INVALID_PRIVKEY = 10,
  /**
   * Database error
   */
  NORDDROP_RES_DB_ERROR = 11,
} norddrop_result;

typedef struct norddrop norddrop;

/**
 * Open FD based on provided content uri.
 * Returns FD on success and -1 on failure
 */
typedef int (*norddrop_fd_fn)(void*, const char*);

/**
 * Fetch file descriptor by the content uri
 */
typedef struct norddrop_fd_cb {
  /**
   * Context to pass to callback.
   * User must ensure safe access of this var from multitheaded context.
   */
  void *ctx;
  /**
   * Function to be called
   */
  norddrop_fd_fn cb;
} norddrop_fd_cb;

typedef void (*norddrop_event_fn)(void*, const char*);

/**
 * Event callback
 */
typedef struct norddrop_event_cb {
  /**
   * Context to pass to callback.
   * User must ensure safe access of this var from multitheaded context.
   */
  void *ctx;
  /**
   * Function to be called
   */
  norddrop_event_fn cb;
} norddrop_event_cb;

typedef void (*norddrop_logger_fn)(void*, enum norddrop_log_level, const char*);

/**
 * Logging callback
 */
typedef struct norddrop_logger_cb {
  /**
   * Context to pass to callback.
   * User must ensure safe access of this var from multitheaded context.
   */
  void *ctx;
  /**
   * Function to be called
   */
  norddrop_logger_fn cb;
} norddrop_logger_cb;

/**
 * Writes the peer's public key into the buffer of length 32.
 * The peer is identifed by IP address passed as string,
 * Returns 0 on success and 1 on failure or missing key
 */
typedef int (*norddrop_pubkey_fn)(void*, const char*, char*);

/**
 * Fetch peer public key callback
 */
typedef struct norddrop_pubkey_cb {
  /**
   * Context to pass to callback.
   * User must ensure safe access of this var from multitheaded context.
   */
  void *ctx;
  /**
   * Function to be called
   */
  norddrop_pubkey_fn cb;
} norddrop_pubkey_cb;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

extern void fortify_source(void);

/**
 * Initialize a new transfer with the provided peer and descriptors
 *
 * # Arguments
 *
 * * `dev` - A pointer to the instance.
 * * `peer` - Peer address.
 * * `descriptors` - JSON descriptors.
 *
 * # Returns
 *
 * A String containing the transfer ID.
 *
 * # Descriptors format
 *
 * Descriptors are provided as an array of JSON objects, with each object
 * containing a "path" and optionally a file descriptor "fd":
 *
 * ```json
 * [
 *   {
 *     "path": "/path/to/file",
 *   },
 *   {
 *     "path": "/path/to/dir",
 *   }
 * ]
 * ```
 *
 * # On Android, due to limitations, we must also accept a file descriptor
 *
 * ```json
 * [
 *   {
 *    "path": "/path/to/file",
 *    "fd": 1234
 *   }
 * ]
 * ```
 *
 * # Safety
 * The pointers provided must be valid
 */
char *norddrop_new_transfer(const struct norddrop *dev, const char *peer, const char *descriptors);

/**
 * Destroy the libdrop instance.
 *
 * # Arguments
 *
 * * `dev` - Pointer to the instance.
 *
 * # Safety
 * This function creates a box with instance pointer and immediately drops it.
 */
void norddrop_destroy(struct norddrop *dev);

/**
 * # Download a file from the peer
 *
 * # Arguments
 *
 * * `dev` - Pointer to the instance
 * * `xfid` - Transfer ID
 * * `fid` - File ID
 * * `dst` - Destination path
 *
 * # Safety
 * The pointers provided must be valid
 */
enum norddrop_result norddrop_download(const struct norddrop *dev,
                                       const char *xfid,
                                       const char *fid,
                                       const char *dst);

/**
 * # Cancel a transfer from either side
 *
 * # Arguments
 *
 * * `dev`: Pointer to the instance
 * * `xfid`: Transfer ID
 *
 * # Safety
 * The pointers provided must be valid
 */
enum norddrop_result norddrop_cancel_transfer(const struct norddrop *dev, const char *xfid);

/**
 * Reject a file from either side
 *
 * # Arguments
 *
 * * `dev`: Pointer to the instance
 * * `xfid`: Transfer ID
 * * `fid`: File ID
 *
 * # Safety
 * The pointers provided should be valid
 */
enum norddrop_result norddrop_reject_file(const struct norddrop *dev,
                                          const char *xfid,
                                          const char *fid);

/**
 * Set FD resolver callback.
 * The callback provides FDs based on URI.
 * This function should be called before `norddrop_start()`, otherwise it will
 * return an error.
 *
 * # Arguments
 *
 * * `dev`: Pointer to the instance
 * * `callback`: Callback structure
 */
enum norddrop_result norddrop_set_fd_resolver_callback(const struct norddrop *dev,
                                                       struct norddrop_fd_cb callback);

/**
 * Start libdrop
 *
 * # Arguments
 *
 * * `dev` - Pointer to the instance
 * * `listen_addr` - Address to listen on
 * * `config` - JSON configuration
 *
 * # Configuration Parameters
 *
 * * `dir_depth_limit` - if the tree contains more levels then the error is
 * returned.
 *
 * * `transfer_file_limit` - when aggregating files from the path, if this
 * limit is reached, an error is returned.
 *
 * * `moose_event_path` - moose database path.
 *
 * * `moose_prod` - moose production flag.
 *
 * * `storage_path` - storage path for persistence engine.
 *
 *
 * # Safety
 * The pointers provided must be valid
 */
enum norddrop_result norddrop_start(const struct norddrop *dev,
                                    const char *listen_addr,
                                    const char *config);

/**
 * Stop norddrop instance
 *
 * # Arguments
 *
 * * `dev` - Pointer to the instance
 */
enum norddrop_result norddrop_stop(const struct norddrop *dev);

/**
 * Purge transfers from the database
 *
 * # Arguments
 *
 * * `dev` - Pointer to the instance
 * * `txids` - JSON array of transfer IDs
 *
 * # Safety
 * The pointers provided must be valid
 */
enum norddrop_result norddrop_purge_transfers(const struct norddrop *dev, const char *txids);

/**
 * Purge transfers from the database until the given timestamp
 *
 *  # Arguments
 *
 * * `dev` - Pointer to the instance
 * * `until_timestamp` - Unix timestamp in seconds
 */
enum norddrop_result norddrop_purge_transfers_until(const struct norddrop *dev,
                                                    long long until_timestamp);

/**
 * Get transfers from the database
 *
 * # Arguments
 *
 * * `dev` - Pointer to the instance
 * * `since_timestamp` - UNIX timestamp in seconds, accepts a value between
 *   -210866760000 and 253402300799
 *
 * # Returns
 *
 * JSON formatted transfers
 *
 * Each transfer and files in it contain history of states that can be used to
 * replay what happened and the last state denotes the current state of the
 * transfer.
 *
 * # Transfer States(same for both incoming and outgoing transfers)
 * - `canceled` - The transfer was successfully canceled by either peer.
 *   Contains indicator of who canceled the transfer.
 * - `failed` - Contains status code of failure. Regular error table should be
 *   consulted for errors.
 * # Incoming File States
 * - `completed` - The file was successfully received and saved to the disk.
 *   Contains the final path of the file.
 * - `failed` - contains status code of failure. Regular error table should be
 *   consulted for errors.
 * - `paused` - The file was paused due to recoverable errors. Most probably
 *   due to network availability.
 * - `pending` - The download was issued for this file and it will proceed when
 *   possible.
 * - `reject` - The file was rejected by the receiver. Contains indicator of
 *   who rejected the file.
 * - `started` - The file was started to be received. Contains the base
 *   directory of the file.
 *
 * Terminal states: `failed`, `completed`, `reject`. Terminal states appear
 * once and it is the final state. Other states might appear multiple times.
 *
 * # Outgoing File States
 * - `completed` - The file was successfully received and saved to the disk.
 *   Contains the final path of the file.
 * - `failed` - contains status code of failure. Regular error table should be
 *   consulted for errors.
 * - `paused` - The file was paused due to recoverable errors. Most probably
 *   due to network availability.
 * - `reject` - The file was rejected by the receiver. Contains indicator of
 *   who rejected the file.
 * - `started` - The file was started to be received. Contains the base
 *   directory of the file.
 *
 * Terminal states: `failed`, `completed`, `reject`. Terminal states appear
 * once and it is the final state. Other states might appear multiple times.
 *
 * Fields `created_at` in the returned JSON refer to the creation time as a
 * UNIX timestamp in milliseconds.
 *
 * # Examples of the same transfer from both sides
 * ## Sender
 *  ```json
 * [
 * {
 *     "id": "0352847a-dfd5-40de-b214-edc1d06e469e",
 *     "created_at": 1698240954430,
 *     "peer_id": "192.168.1.3",
 *     "states": [
 *         {
 *             "created_at": 1698240956418,
 *             "state": "cancel",
 *             "by_peer": true
 *         }
 *     ],
 *     "type": "outgoing",
 *     "paths": [
 *         {
 *             "created_at": 1698240954430,
 *             "transfer_id": "0352847a-dfd5-40de-b214-edc1d06e469e",
 *             "base_path": "/tmp",
 *             "relative_path": "testfile-big",
 *             "file_id": "ESDW8PFTBoD8UYaqxMSWp6FBCZN3SKnhyHFqlhrdMzU",
 *             "bytes": 10485760,
 *             "bytes_sent": 10485760,
 *             "states": [
 *                 {
 *                     "created_at": 1698240955416,
 *                     "state": "started",
 *                     "bytes_sent": 0
 *                 },
 *                 {
 *                     "created_at": 1698240955856,
 *                     "state": "completed"
 *                 }
 *             ]
 *         }
 *     ]
 * }
 * ]
 * ```
 *
 * ## Receiver
 * ```json
 * [
 * {
 *     "id": "0352847a-dfd5-40de-b214-edc1d06e469e",
 *     "created_at": 1698240954437,
 *     "peer_id": "192.168.1.2",
 *     "states": [
 *         {
 *             "created_at": 1698240956417,
 *             "state": "cancel",
 *             "by_peer": false
 *         }
 *     ],
 *     "type": "incoming",
 *     "paths": [
 *         {
 *             "created_at": 1698240954437,
 *             "transfer_id": "0352847a-dfd5-40de-b214-edc1d06e469e",
 *             "relative_path": "testfile-big",
 *             "file_id": "ESDW8PFTBoD8UYaqxMSWp6FBCZN3SKnhyHFqlhrdMzU",
 *             "bytes": 10485760,
 *             "bytes_received": 10485760,
 *             "states": [
 *                 {
 *                     "created_at": 1698240955415,
 *                     "state": "pending",
 *                     "base_dir": "/tmp/received"
 *                 },
 *                 {
 *                     "created_at": 1698240955415,
 *                     "state": "started",
 *                     "bytes_received": 0
 *                 },
 *                 {
 *                     "created_at": 1698240955855,
 *                     "state": "completed",
 *                     "final_path": "/tmp/received/testfile-big"
 *                 }
 *             ]
 *         }
 *     ]
 * }
 * ]
 * ```
 */
char *norddrop_get_transfers_since(const struct norddrop *dev, long long since_timestamp);

/**
 * Removes a single transfer file from the database. The file must be rejected
 * beforehand, otherwise the error is returned.
 *
 *  # Arguments
 *
 * * `dev`: Pointer to the instance
 * * `xfid`: Transfer ID
 * * `fid`: File ID
 *
 * # Safety
 * The pointers provided should be valid
 */
enum norddrop_result norddrop_remove_transfer_file(const struct norddrop *dev,
                                                   const char *xfid,
                                                   const char *fid);

/**
 * Create a new instance of norddrop. This is a required step to work
 * with API further
 *
 * # Arguments
 *
 * * `dev` - Pointer to the pointer to the instance. The pointer will be
 *   allocated by the function and should be freed by the caller using
 *   norddrop_destroy()
 * * `event_cb` - Event callback
 * * `log_level` - Log level
 * * `logger_cb` - Logger callback
 * * `pubkey_cb` - Fetch peer public key callback. It is used to request
 * the app to provide the peer’s public key or the node itself. The callback
 * provides two parameters, `const char *ip` which is a string
 * representation of the peer’s IP address, and `char *pubkey` which is
 * preallocated buffer of size 32 into which the app should write the public
 * key as bytes. The app returns the status of the callback call, 0 on
 * success and a non-zero value to indicate that the key could not be
 * provided. Note that it’s not BASE64, it must be decoded if it is beforehand.
 * * `privkey` - 32bytes private key. Note that it’s not BASE64, it must
 * be decoded if it is beforehand.
 *
 * # Safety
 * The pointers provided must be valid as well as callback functions
 */
enum norddrop_result norddrop_new(struct norddrop **dev,
                                  struct norddrop_event_cb event_cb,
                                  enum norddrop_log_level log_level,
                                  struct norddrop_logger_cb logger_cb,
                                  struct norddrop_pubkey_cb pubkey_cb,
                                  const char *privkey);

/**
 * Refresh connections. Should be called when anything about the network
 * changes that might affect connections. Also when peer availability has
 * changed. This will kick-start the automated retries for all transfers.
 *
 * # Arguments
 *
 * * `dev` - A pointer to the instance.
 *
 * # Safety
 * The pointers provided should be valid
 */
enum norddrop_result norddrop_network_refresh(const struct norddrop *dev);

void __norddrop_force_export(enum norddrop_result,
                             struct norddrop_event_cb,
                             struct norddrop_logger_cb,
                             struct norddrop_pubkey_cb,
                             struct norddrop_fd_cb);

/**
 * Get the version of the library
 *
 * # Returns
 *
 * Version string
 */
const char *norddrop_version(void);

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus

#endif /* NORDDROP_H */
