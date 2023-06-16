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

void __norddrop_force_export(enum norddrop_result,
                             struct norddrop_event_cb,
                             struct norddrop_logger_cb,
                             struct norddrop_pubkey_cb);

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus

#endif /* NORDDROP_H */
