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
 */
char *norddrop_new_transfer(const struct norddrop *dev, const char *peer, const char *descriptors);

/**
 * Destroy the libdrop instance.
 *
 * # Arguments
 *
 * * `dev` - Pointer to the instance.
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
 */
enum norddrop_result norddrop_cancel_transfer(const struct norddrop *dev, const char *xfid);

/**
 * # Cancel a file from either side
 *
 * # Arguments
 *
 * * `dev`: Pointer to the instance
 * * `xfid`: Transfer ID
 * * `fid`: File ID
 */
enum norddrop_result norddrop_cancel_file(const struct norddrop *dev,
                                          const char *xfid,
                                          const char *fid);

/**
 * Reject a file from either side
 *
 * # Arguments
 *
 * * `dev` -   Pointer to the instance
 * * `xfid` -  Transfer ID
 * * `fid` -   File ID
 */
enum norddrop_result norddrop_reject_file(const struct norddrop *dev,
                                          const char *xfid,
                                          const char *fid);

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
 * * `req_connection_timeout_ms` - timeout value used in connecting to the
 *   peer.
 * The formula for retrying is: starting from 0.2 seconds we double it
 * each time until we cap at req_connection_timeout_ms / 10. This is useful
 * when the peer is not responding at all.
 *
 * * `transfer_idle_lifetime_ms` - this timeout plays a role in an already
 * established transfer as sometimes one peer might go offline with no notice.
 * This timeout controls the amount of time we will wait for any action from
 * the peer and after that, we will fail the transfer.
 *
 * * `moose_event_path` - moose database path.
 *
 * * `storage_path` - storage path for persistence engine.
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
 * * `since_timestamp` - Timestamp in seconds
 *
 * # Returns
 *
 * JSON formatted transfers
 *
 * # JSON example from the sender side
 *  ```json
 * {
 *      "id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
 *      "peer_id": "192.168.0.0",
 *      "created_at": 1686651025988,
 *      "states": [
 *          {
 *              "created_at": 1686651026008,
 *              "state": "cancel",
 *              "by_peer": true
 *          }
 *      ],
 *      "type": "outgoing",
 *      "paths": [
 *          {
 *              "transfer_id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
 *              "base_path": "/home/user/Pictures",
 *              "relative_path": "doggo.jpg",
 *              "file_id": "Unu_l4PVyu15-RsdVL9IOQvaKQdqcqUy7F9EpvP-CrY",
 *              "bytes": 29852,
 *              "created_at": 1686651025988,
 *              "states": [
 *                  {
 *                      "created_at": 1686651025991,
 *                      "state": "pending"
 *                  },
 *                  {
 *                      "created_at": 1686651025997,
 *                      "state": "started",
 *                      "bytes_sent": 0
 *                  },
 *                  {
 *                      "created_at": 1686651026002,
 *                      "state": "completed"
 *                  }
 *              ]
 *          }
 *      ]
 *  }
 * ```
 *
 * # JSON example from the receiver side
 * ```json
 * {
 *     "id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
 *     "peer_id": "172.17.0.1",
 *     "created_at": 1686651025988,
 *     "states": [
 *         {
 *             "created_at": 1686651026007,
 *             "state": "cancel",
 *             "by_peer": false
 *         }
 *     ],
 *     "type": "outgoing",
 *     "paths": [
 *         {
 *             "transfer_id": "b49fc2f8-ce2d-41ac-a081-96a4d760899e",
 *             "relative_path": "doggo.jpg",
 *             "file_id": "Unu_l4PVyu15-RsdVL9IOQvaKQdqcqUy7F9EpvP-CrY",
 *             "bytes": 29852,
 *             "created_at": 1686651025988,
 *             "states": [
 *                 {
 *                     "created_at": 1686651025992,
 *                     "state": "pending"
 *                 },
 *                 {
 *                     "created_at": 1686651026000,
 *                     "state": "started",
 *                     "base_dir": "/root",
 *                     "bytes_received": 0
 *                 },
 *                 {
 *                     "created_at": 1686651026003,
 *                     "state": "completed",
 *                     "final_path": "/root/doggo.jpg"
 *                 }
 *             ]
 *         }
 *     ]
 * }
 * ```
 */
char *norddrop_get_transfers_since(const struct norddrop *dev, long long since_timestamp);

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
 */
enum norddrop_result norddrop_new(struct norddrop **dev,
                                  struct norddrop_event_cb event_cb,
                                  enum norddrop_log_level log_level,
                                  struct norddrop_logger_cb logger_cb,
                                  struct norddrop_pubkey_cb pubkey_cb,
                                  const char *privkey);

void __norddrop_force_export(enum norddrop_result,
                             struct norddrop_event_cb,
                             struct norddrop_logger_cb,
                             struct norddrop_pubkey_cb);

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
