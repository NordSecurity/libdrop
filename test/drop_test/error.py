from enum import IntEnum


class Error(IntEnum):
    CANCELED = (1,)
    BAD_PATH = (2,)
    BAD_FILE = (3,)
    BAD_TRANSFER = (7,)
    BAD_TRANSFER_STATE = (8,)
    BAD_FILE_ID = (9,)
    IO = (15,)
    TRANSFER_LIMITS_EXCEEDED = (20,)
    MISMATCHED_SIZE = (21,)
    INVALID_ARGUMENT = (23,)
    ADDR_IN_USE = (27,)
    FILE_MODIFIED = (28,)
    FILENAME_TOO_LONG = (29,)
    AUTHENTICATION_FAILED = (30,)
    STORAGE_ERROR = (31,)
    DB_LOST = (32,)
    FILE_CHECKSUM_MISMATCH = (33,)
    FILE_REJECTED = (34,)
    FILE_FAILED = (35,)
    FILE_FINISHED = (36,)


class ReturnCodes(IntEnum):
    OK = (0,)
    ERROR = (1,)
    INVALID_STRING = (2,)
    BAD_INPUT = (3,)
    JSON_PARSE = (4,)
    TRANSFER_CREATE = (5,)
    NOT_STARTED = (6,)
    ADDR_IN_USE = (7,)
    INSTANCE_START = (8,)
    INSTANCE_STOP = (9,)
    INVALID_PRIVKEY = (10,)
    DB_ERROR = (11,)
