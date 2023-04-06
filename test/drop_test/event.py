from __future__ import annotations
from pathlib import Path
from collections import Counter
import typing

from drop_test.error import Error

UUIDS: typing.List[str] = []


def print_uuid(slot: int) -> str:
    uuid: str = "MISSING"
    if slot < len(UUIDS):
        uuid = UUIDS[slot]

    return f"{uuid} (slot: {slot})"


class Event:
    def __init__(self):
        raise Exception("Base Event class should not be initialized")


class File:
    def __init__(self, path: str, size: int, children: typing.Set[File] = set()):
        self._path = path
        self._size = size
        self._children = children

    def __eq__(lhs, rhs):
        if not isinstance(rhs, File):
            return NotImplemented

        return set(lhs.flatten()) == set(rhs.flatten())

    def flatten(self, parent="", collection=None):
        if collection is None:
            collection = []

        collection.append((parent, self._path, self._size))

        for child in self._children:
            child.flatten(self._path, collection)

        return collection

    def __hash__(self):
        return hash(str(self._path))

    def __repr__(self):
        return f"File(path={self._path}, size={self._size}, children={self._children})"


class Queued(Event):
    def __init__(self, uuid_slot: int, files: typing.Set[File]):
        self._uuid_slot = uuid_slot
        self._files: typing.Set[File] = files

    def __eq__(self, rhs) -> bool:
        if not isinstance(rhs, Queued):
            return NotImplemented

        if self._uuid_slot != rhs._uuid_slot or Counter(self._files) != Counter(
            rhs._files
        ):
            return False

        return True

    def __str__(self):
        return f"Queued(uuid={print_uuid(self._uuid_slot)}, files={self._files})"


class Receive(Event):
    def __init__(self, uuid_slot: int, peer: str, files: typing.Set[File]):
        self._uuid_slot: int = uuid_slot
        self._peer: str = peer
        self._files: typing.Set[File] = files

    def __eq__(self, rhs) -> bool:
        if not isinstance(rhs, Receive):
            return NotImplemented

        if (
            self._uuid_slot != rhs._uuid_slot
            or self._peer != rhs._peer
            or Counter(self._files) != Counter(rhs._files)
        ):
            return False

        return True

    def __str__(self):
        return f"Receive(peer={self._peer}, uuid={print_uuid(self._uuid_slot)}, files={self._files})"


class Start(Event):
    def __init__(self, uuid_slot: int, file: str):
        self._uuid_slot = uuid_slot
        self._file = file

    def __eq__(self, rhs):
        if not isinstance(rhs, Start):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        return True

    def __str__(self):
        return f"Start(transfer={print_uuid(self._uuid_slot)}, file={self._file})"


class Progress(Event):
    def __init__(
        self, uuid_slot: int, file: str, transferred: typing.Optional[int] = None
    ):
        self._uuid_slot = uuid_slot
        self._file = file
        self._transferred = transferred

    def __eq__(self, rhs) -> bool:
        if not isinstance(rhs, Progress):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        if self._transferred is not None and rhs._transferred is not None:
            if self._transferred != rhs._transferred:
                return False

        return True

    def __str__(self):
        return f"Progress(transfer={print_uuid(self._uuid_slot)}, file={self._file}, transfered={self._transferred})"


class FinishTransferCanceled(Event):
    def __init__(self, uuid_slot: int, by_peer: bool):
        self._uuid_slot = uuid_slot
        self._by_peer = by_peer

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishTransferCanceled):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._by_peer != rhs._by_peer:
            return False

        return True

    def __str__(self):
        return f"FinishTransferCanceled(transfer={print_uuid(self._uuid_slot)}, by_peer={self._by_peer})"


class FinishFileUploaded(Event):
    def __init__(self, uuid_slot: int, file: str):
        self._uuid_slot = uuid_slot
        self._file = file

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileUploaded):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False

        return True

    def __str__(self):
        return f"FinishFileUploaded(transfer={print_uuid(self._uuid_slot)}, file={self._file})"


class FinishFileDownloaded(Event):
    def __init__(self, uuid_slot: int, file: str, final_path: str):
        self._uuid_slot = uuid_slot
        self._file = file
        self._final_path = final_path

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileDownloaded):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False
        if self._final_path != rhs._final_path:
            return False

        return True

    def __str__(self):
        return f"FinishFileDownloaded(transfer={print_uuid(self._uuid_slot)}, file={self._file}, final_path={self._final_path})"


class FinishFileCanceled(Event):
    def __init__(self, uuid_slot: int, file: str, by_peer: bool):
        self._uuid_slot = uuid_slot
        self._file = file
        self._by_peer = by_peer

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileCanceled):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False
        if self._by_peer != rhs._by_peer:
            return False

        return True

    def __str__(self):
        return f"FinishFileCanceled(transfer={print_uuid(self._uuid_slot)}, file={self._file}, by_peer={self._by_peer})"


class FinishFileFailed(Event):
    def __init__(self, uuid_slot: int, file: str, status: int):
        self._uuid_slot = uuid_slot
        self._file = file
        self._status = status

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFileFailed):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._file != rhs._file:
            return False
        if self._status != rhs._status:
            return False

        return True

    def __str__(self):
        return f"FinishFileFailed(transfer={print_uuid(self._uuid_slot)}, file={self._file}, status={self._status})"


class FinishFailedTransfer(Event):
    def __init__(self, uuid_slot: int, status: int):
        self._uuid_slot = uuid_slot
        self._status = status

    def __eq__(self, rhs):
        if not isinstance(rhs, FinishFailedTransfer):
            return False
        if self._uuid_slot != rhs._uuid_slot:
            return False
        if self._status != rhs._status:
            return False

        return True

    def __str__(self):
        return f"FinishFailedTransfer(transfer={print_uuid(self._uuid_slot)}, status={self._status})"


class Panic(Event):
    def __init__(self, info: str):
        self._info = info

    def __eq__(self, rhs):
        if not isinstance(rhs, Panic):
            return False
        if self._info != rhs._info:
            return False
        return True

    def __str__(self):
        return f"Panic(info={self._info})"
