from drop_test import action, event
from drop_test.scenario import Scenario, ActionList
from drop_test.error import Error

# We are using the transfer slots instead of UUIDS.
# Each call to `action.NewTransfer` or the `Receive` event inserts the transfer UUID into the next slot - starting from 0

scenarios = [
    Scenario(
        "scenario1",
        "Send one file to a peer, expect it to be transferred",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-big",
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-big",
                            "testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario2",
        "Send two files one by one",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer("172.20.0.15", "/tmp/testfile-small"),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        )
                    ),
                    action.NewTransfer("172.20.0.15", "/tmp/testfile-big"),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(1, "testfile-big")),
                    action.Wait(
                        event.FinishFileUploaded(
                            1,
                            "testfile-big",
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        1,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(1, "testfile-big")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            1,
                            "testfile-big",
                            "testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-small", 1048576),
                            event.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario3",
        "Send two files in parallel",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer("172.20.0.15", "/tmp/testfile-big"),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        ),
                    ),
                    action.NewTransfer("172.20.0.15", "/tmp/testfile-small"),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File("testfile-small", 1048576),
                            },
                        ),
                    ),
                    action.WaitRacy(
                        sum(
                            [
                                [
                                    event.Start(
                                        0,
                                        "testfile-big",
                                    ),
                                    event.Start(
                                        1,
                                        "testfile-small",
                                    ),
                                    event.FinishFileUploaded(
                                        0,
                                        "testfile-big",
                                    ),
                                    event.FinishFileUploaded(
                                        1,
                                        "testfile-small",
                                    ),
                                ],
                            ],
                            [],
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.WaitRacy(
                        [
                            event.Receive(
                                0,
                                "172.20.0.5",
                                {
                                    event.File("testfile-big", 10485760),
                                },
                            ),
                            event.Receive(
                                1,
                                "172.20.0.5",
                                {
                                    event.File("testfile-small", 1048576),
                                },
                            ),
                        ]
                    ),
                    action.Download(
                        1,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.WaitRacy(
                        sum(
                            [
                                [
                                    event.Start(
                                        0,
                                        "testfile-big",
                                    ),
                                    event.FinishFileDownloaded(
                                        0,
                                        "testfile-big",
                                        "testfile-big",
                                    ),
                                    event.Start(
                                        1,
                                        "testfile-small",
                                    ),
                                    event.FinishFileDownloaded(
                                        1,
                                        "testfile-small",
                                        "testfile-small",
                                    ),
                                ],
                            ],
                            [],
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-small", 1048576),
                            event.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-1",
        "Send a request with one file, cancel the request from the sender side once it starts downloading",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.CancelTransferRequest(0),
                    action.Wait(event.FinishTransferCanceled(0, False)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(event.FinishTransferCanceled(0, True)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-2",
        "Send a request with one file, cancel the request from the sender side before downloading",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.CancelTransferRequest(0),
                    action.Wait(event.FinishTransferCanceled(0, False)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishTransferCanceled(0, True),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-3",
        "Send a request with one file, cancel from the receiver side once the download begins",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishTransferCanceled(0, True),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.CancelTransferRequest(0),
                    action.Wait(
                        event.FinishTransferCanceled(0, False),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-4",
        "Send a request with one file, cancel from the receiver side before download begins",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.FinishTransferCanceled(0, True)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.CancelTransferRequest(0),
                    action.Wait(
                        event.FinishTransferCanceled(0, False),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-5",
        "Send one file, cancel the file from the receiver, before the transfer is started",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.CancelTransferFile(
                        0,
                        "testfile-big",
                    ),
                    # No event since the download was not started yet
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-6",
        "Send one file, cancel the file from the receiver once the download has started",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFileCanceled(0, "testfile-big", True),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.CancelTransferFile(0, "testfile-big"),
                    action.Wait(
                        event.FinishFileCanceled(
                            0,
                            "testfile-big",
                            False,
                        ),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-7",
        "Send one file, the receiver downloads it fully, both sides receive TransferDownloaded/TransferUploaded, then receiver issues cancel_file() - expect nothing to happen",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1024 * 1024),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-small", 1024 * 1024),
                        ],
                    ),
                    action.CancelTransferFile(0, "testfile-small"),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-8",
        "Send one file, the receiver downloads it fully, both sides receive TransferDownloaded/TransferUploaded, then sender issues cancel_file() - expect nothing to happen",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1024 * 1024),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        )
                    ),
                    action.CancelTransferFile(0, "testfile-small"),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-small", 1024 * 1024),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario4-9",
        "Start transfer with multiple files, cancel the transfer from the receiver once the download has started",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/nested/big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    "big",
                                    0,
                                    {
                                        event.File("testfile-01", 10485760),
                                        event.File("testfile-02", 10485760),
                                    },
                                )
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                "big/testfile-01",
                            ),
                            event.Start(
                                0,
                                "big/testfile-02",
                            ),
                        ]
                    ),
                    action.Wait(
                        event.FinishTransferCanceled(0, True),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File(
                                    "big",
                                    0,
                                    {
                                        event.File("testfile-01", 10485760),
                                        event.File("testfile-02", 10485760),
                                    },
                                )
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "big/testfile-01",
                        "/tmp/received",
                    ),
                    action.Download(
                        0,
                        "big/testfile-02",
                        "/tmp/received",
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                "big/testfile-01",
                            ),
                            event.Start(
                                0,
                                "big/testfile-02",
                            ),
                        ]
                    ),
                    action.CancelTransferRequest(0),
                    action.Wait(
                        event.FinishTransferCanceled(
                            0,
                            False,
                        ),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario5",
        "Try to send two files to an offline peer",
        {
            "ren": ActionList(
                [
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.IO,
                        )
                    ),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.FinishFailedTransfer(
                            1,
                            Error.IO,
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario6",
        "Send nested directory, expect it to be transferred fully",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/deep",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    "deep",
                                    0,
                                    {
                                        event.File(
                                            "another-path",
                                            0,
                                            {
                                                event.File("file3.ext3", 1048576),
                                                event.File("file4.ext4", 1048576),
                                            },
                                        ),
                                        event.File(
                                            "path",
                                            0,
                                            {
                                                event.File("file1.ext1", 1048576),
                                                event.File("file2.ext2", 1048576),
                                            },
                                        ),
                                    },
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/path/file1.ext1",
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "deep/path/file1.ext1",
                        ),
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/path/file2.ext2",
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "deep/path/file2.ext2",
                        ),
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/another-path/file3.ext3",
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "deep/another-path/file3.ext3",
                        ),
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/another-path/file4.ext4",
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "deep/another-path/file4.ext4",
                        ),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File(
                                    "deep",
                                    0,
                                    {
                                        event.File(
                                            "another-path",
                                            0,
                                            {
                                                event.File("file3.ext3", 1048576),
                                                event.File("file4.ext4", 1048576),
                                            },
                                        ),
                                        event.File(
                                            "path",
                                            0,
                                            {
                                                event.File("file1.ext1", 1048576),
                                                event.File("file2.ext2", 1048576),
                                            },
                                        ),
                                    },
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "deep/path/file1.ext1",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/path/file1.ext1",
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "deep/path/file1.ext1",
                            "file1.ext1",
                        ),
                    ),
                    action.Download(
                        0,
                        "deep/path/file2.ext2",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/path/file2.ext2",
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "deep/path/file2.ext2",
                            "file2.ext2",
                        ),
                    ),
                    action.Download(
                        0,
                        "deep/another-path/file3.ext3",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/another-path/file3.ext3",
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "deep/another-path/file3.ext3",
                            "file3.ext3",
                        ),
                    ),
                    action.Download(
                        0,
                        "deep/another-path/file4.ext4",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "deep/another-path/file4.ext4",
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "deep/another-path/file4.ext4",
                            "file4.ext4",
                        ),
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/deep/path/file1.ext1", 1048576),
                            event.File("/tmp/received/deep/path/file2.ext2", 1048576),
                            event.File(
                                "/tmp/received/deep/another-path/file3.ext3", 1048576
                            ),
                            event.File(
                                "/tmp/received/deep/another-path/file4.ext4", 1048576
                            ),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario7",
        "Send one file to another peer. Pre-open the file and pass a descriptor",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransferWithFD(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(0, "testfile-small"),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(0, "testfile-small"),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1048576),
                            },
                        ),
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(0, "testfile-small"),
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        ),
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario8",
        "Send two identical files one by one, expect no overwrites to happen",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer("172.20.0.15", "/tmp/testfile-small"),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(event.FinishFileUploaded(0, "testfile-small")),
                    action.NewTransfer("172.20.0.15", "/tmp/testfile-small"),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Wait(event.Start(1, "testfile-small")),
                    action.Wait(event.FinishFileUploaded(1, "testfile-small")),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Download(
                        1,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(1, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            1,
                            "testfile-small",
                            "testfile-small(1)",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-small", 1048576),
                            event.File("/tmp/received/testfile-small(1)", 1048576),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario9",
        "Send two identical files with extensions one by one, expect no overwrites to happen",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer("172.20.0.15", "/tmp/deep/path/file1.ext1"),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("file1.ext1", 1048576),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "file1.ext1")),
                    action.Wait(event.FinishFileUploaded(0, "file1.ext1")),
                    action.NewTransfer("172.20.0.15", "/tmp/deep/path/file1.ext1"),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File("file1.ext1", 1048576),
                            },
                        )
                    ),
                    action.Wait(event.Start(1, "file1.ext1")),
                    action.Wait(event.FinishFileUploaded(1, "file1.ext1")),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("file1.ext1", 1048576),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "file1.ext1",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "file1.ext1")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "file1.ext1",
                            "file1.ext1",
                        )
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "172.20.0.5",
                            {
                                event.File("file1.ext1", 1048576),
                            },
                        )
                    ),
                    action.Download(
                        1,
                        "file1.ext1",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(1, "file1.ext1")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            1,
                            "file1.ext1",
                            "file1(1).ext1",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/file1.ext1", 1048576),
                            event.File("/tmp/received/file1(1).ext1", 1048576),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario10-1",
        "Start file transfer then stop drop instance on the sender side",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Stop(),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.CANCELED,
                        )
                    ),
                    action.NoEvent(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.WS_SERVER,
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario10-2",
        "Start file transfer then stop drop instance on the receiver side",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.WS_CLIENT,
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Stop(),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.CANCELED,
                        )
                    ),
                    action.NoEvent(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario10-3",
        "Start file transfer to offline peeer, then stop immediately before the connection fails",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(latency="10000ms"),
                    action.NewTransfer(
                        "172.20.0.100",
                        "/tmp/testfile-big",
                    ),
                    action.Stop(),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.CANCELED,
                        )
                    ),
                    action.NoEvent(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario11",
        "Send a couple of file simultaneously and see if libdrop freezes",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    # fmt: off
                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-01"),
                    action.Wait(event.Queued(0, { event.File("testfile-bulk-01", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-02"),
                    action.Wait(event.Queued(1, { event.File("testfile-bulk-02", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-03"),
                    action.Wait(event.Queued(2, { event.File("testfile-bulk-03", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-04"),
                    action.Wait(event.Queued(3, { event.File("testfile-bulk-04", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-05"),
                    action.Wait(event.Queued(4, { event.File("testfile-bulk-05", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-06"),
                    action.Wait(event.Queued(5, { event.File("testfile-bulk-06", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-07"),
                    action.Wait(event.Queued(6, { event.File("testfile-bulk-07", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-08"),
                    action.Wait(event.Queued(7, { event.File("testfile-bulk-08", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-09"),
                    action.Wait(event.Queued(8, { event.File("testfile-bulk-09", 10485760), })),

                    action.NewTransfer("172.20.0.15", "/tmp/testfile-bulk-10"),
                    action.Wait(event.Queued(9, { event.File("testfile-bulk-10", 10485760), })),

                    # fmt: on

                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Start(0, "testfile-bulk-01"),
                            event.Start(1, "testfile-bulk-02"),
                            event.Start(2, "testfile-bulk-03"),
                            event.Start(3, "testfile-bulk-04"),
                            event.Start(4, "testfile-bulk-05"),
                            event.Start(5, "testfile-bulk-06"),
                            event.Start(6, "testfile-bulk-07"),
                            event.Start(7, "testfile-bulk-08"),
                            event.Start(8, "testfile-bulk-09"),
                            event.Start(9, "testfile-bulk-10"),

                            event.FinishFileUploaded(0, "testfile-bulk-01"),
                            event.FinishFileUploaded(1, "testfile-bulk-02"),
                            event.FinishFileUploaded(2, "testfile-bulk-03"),
                            event.FinishFileUploaded(3, "testfile-bulk-04"),
                            event.FinishFileUploaded(4, "testfile-bulk-05"),
                            event.FinishFileUploaded(5, "testfile-bulk-06"),
                            event.FinishFileUploaded(6, "testfile-bulk-07"),
                            event.FinishFileUploaded(7, "testfile-bulk-08"),
                            event.FinishFileUploaded(8, "testfile-bulk-09"),
                            event.FinishFileUploaded(9, "testfile-bulk-10"),
                        ]
                    ),
                    # fmt: on
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Receive(0, "172.20.0.5", { event.File("testfile-bulk-01", 10485760), }),
                            event.Receive(1, "172.20.0.5", { event.File("testfile-bulk-02", 10485760), }),
                            event.Receive(2, "172.20.0.5", { event.File("testfile-bulk-03", 10485760), }),
                            event.Receive(3, "172.20.0.5", { event.File("testfile-bulk-04", 10485760), }),
                            event.Receive(4, "172.20.0.5", { event.File("testfile-bulk-05", 10485760), }),
                            event.Receive(5, "172.20.0.5", { event.File("testfile-bulk-06", 10485760), }),
                            event.Receive(6, "172.20.0.5", { event.File("testfile-bulk-07", 10485760), }),
                            event.Receive(7, "172.20.0.5", { event.File("testfile-bulk-08", 10485760), }),
                            event.Receive(8, "172.20.0.5", { event.File("testfile-bulk-09", 10485760), }),
                            event.Receive(9, "172.20.0.5", { event.File("testfile-bulk-10", 10485760), }),
                        ]
                    ),
                    # fmt: on

                    # fmt: off
                    action.Download(0, "testfile-bulk-01", "/tmp/received"),
                    action.Download(1, "testfile-bulk-02", "/tmp/received"),
                    action.Download(2, "testfile-bulk-03", "/tmp/received"),
                    action.Download(3, "testfile-bulk-04", "/tmp/received"),
                    action.Download(4, "testfile-bulk-05", "/tmp/received"),
                    action.Download(5, "testfile-bulk-06", "/tmp/received"),
                    action.Download(6, "testfile-bulk-07", "/tmp/received"),
                    action.Download(7, "testfile-bulk-08", "/tmp/received"),
                    action.Download(8, "testfile-bulk-09", "/tmp/received"),
                    action.Download(9, "testfile-bulk-10", "/tmp/received"),
                    # fmt: on

                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Start(0, "testfile-bulk-01"),
                            event.Start(1, "testfile-bulk-02"),
                            event.Start(2, "testfile-bulk-03"),
                            event.Start(3, "testfile-bulk-04"),
                            event.Start(4, "testfile-bulk-05"),
                            event.Start(5, "testfile-bulk-06"),
                            event.Start(6, "testfile-bulk-07"),
                            event.Start(7, "testfile-bulk-08"),
                            event.Start(8, "testfile-bulk-09"),
                            event.Start(9, "testfile-bulk-10"),

                            event.FinishFileDownloaded(0, "testfile-bulk-01", "testfile-bulk-01"),
                            event.FinishFileDownloaded(1, "testfile-bulk-02", "testfile-bulk-02"),
                            event.FinishFileDownloaded(2, "testfile-bulk-03", "testfile-bulk-03"),
                            event.FinishFileDownloaded(3, "testfile-bulk-04", "testfile-bulk-04"),
                            event.FinishFileDownloaded(4, "testfile-bulk-05", "testfile-bulk-05"),
                            event.FinishFileDownloaded(5, "testfile-bulk-06", "testfile-bulk-06"),
                            event.FinishFileDownloaded(6, "testfile-bulk-07", "testfile-bulk-07"),
                            event.FinishFileDownloaded(7, "testfile-bulk-08", "testfile-bulk-08"),
                            event.FinishFileDownloaded(8, "testfile-bulk-09", "testfile-bulk-09"),
                            event.FinishFileDownloaded(9, "testfile-bulk-10", "testfile-bulk-10"),
                        ]
                    ),
                    # fmt: on
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario12-1",
        "Transfer file to two clients simultaneously",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransferWithFD(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.NewTransferWithFD(
                        "172.20.0.25",
                        "/tmp/testfile-big",
                    ),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File("testfile-big", 10485760),
                                },
                            ),
                            event.Queued(
                                1,
                                {
                                    event.File("testfile-big", 10485760),
                                },
                            ),
                            event.Start(1, "testfile-big"),
                            event.Start(0, "testfile-big"),
                            event.FinishFileUploaded(
                                1,
                                "testfile-big",
                            ),
                            event.FinishFileUploaded(
                                0,
                                "testfile-big",
                            ),
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "george": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received/stimpy",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-big",
                            "testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/stimpy/testfile-big", 10485760),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received/george",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-big",
                            "testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/george/testfile-big", 10485760),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario12-2",
        "Transfer file to two peers with the same file descriptor",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.MultipleNewTransfersWithSameFD(
                        [
                            "172.20.0.15",
                            "172.20.0.25",
                        ],
                        "/tmp/testfile-small",
                    ),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File("testfile-small", 1024 * 1024),
                                },
                            ),
                            event.Queued(
                                1,
                                {
                                    event.File("testfile-small", 1024 * 1024),
                                },
                            ),
                        ]
                    ),
                    action.WaitRacy(
                        [
                            event.Start(0, "testfile-small"),
                            event.Start(1, "testfile-small"),
                            event.FinishFileUploaded(
                                0,
                                "testfile-small",
                            ),
                            event.FinishFileUploaded(
                                1,
                                "testfile-small",
                            ),
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received/stimpy",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File(
                                "/tmp/received/stimpy/testfile-small", 1024 * 1024
                            ),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "george": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received/george",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File(
                                "/tmp/received/george/testfile-small", 1024 * 1024
                            ),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario13-1",
        "Transfer file with the same name as symlink in destination directory, expect appending `(1)` suffix",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        ),
                    ),
                    action.Wait(
                        event.Start(0, "testfile-small"),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        ),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received/symtest-files",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small(1)",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File(
                                "/tmp/received/symtest-files/testfile-small(1)",
                                1 * 1024 * 1024,
                            ),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario13-2",
        "Transfer file into simlinked directory, expect error",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        ),
                    ),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.WS_CLIENT,
                        ),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received/symtest-dir",
                    ),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            "testfile-small",
                            Error.BAD_PATH,
                        )
                    ),
                    action.CheckFileDoesNotExist(
                        [
                            "/tmp/symtest-dir/testfile-small",
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario14-1",
        "Fail the first download call, then call again with proper arguments. Expect only one `Start` event on the sender",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        ),
                    ),
                    action.Wait(
                        event.Start(0, "testfile-small"),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        ),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small-xd",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            "testfile-small-xd",
                            Error.BAD_FILE_ID,
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File(
                                "/tmp/received/testfile-small",
                                1 * 1024 * 1024,
                            ),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario14-2",
        "Fail new_transfer call, then call again with proper arguments. Expect only one `Receive` event on the sender",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransferFails(
                        "172.20.0.15",
                        "/tmp/testfile-small-xd",
                    ),
                    action.NoEvent(duration=2),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        ),
                    ),
                    action.Wait(
                        event.Start(0, "testfile-small"),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        ),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1 * 1024 * 1024),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File(
                                "/tmp/received/testfile-small",
                                1 * 1024 * 1024,
                            ),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario15-1",
        "Repeated file download",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile-small",
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-small", 1048576),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small",
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-small",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-small")),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile-small",
                            "testfile-small(1)",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File("/tmp/received/testfile-small", 1048576),
                            event.File("/tmp/received/testfile-small(1)", 1048576),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario15-2",
        "Repeated file download with complicated extension, expect appending (1), no reanme or other weird stuff",
        {
            "ren": ActionList(
                [
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile.small.with.complicated.extension",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    "testfile.small.with.complicated.extension", 1048576
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "testfile.small.with.complicated.extension",
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile.small.with.complicated.extension",
                        )
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "testfile.small.with.complicated.extension",
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "testfile.small.with.complicated.extension",
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File(
                                    "testfile.small.with.complicated.extension", 1048576
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile.small.with.complicated.extension",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "testfile.small.with.complicated.extension",
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile.small.with.complicated.extension",
                            "testfile.small.with.complicated.extension",
                        )
                    ),
                    action.Download(
                        0,
                        "testfile.small.with.complicated.extension",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "testfile.small.with.complicated.extension",
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "testfile.small.with.complicated.extension",
                            "testfile.small.with.complicated(1).extension",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            event.File(
                                "/tmp/received/testfile.small.with.complicated.extension",
                                1048576,
                            ),
                            event.File(
                                "/tmp/received/testfile.small.with.complicated(1).extension",
                                1048576,
                            ),
                        ],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario16",
        "Activate connection timeout",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(latency="10000ms"),
                    action.NewTransfer(
                        "172.20.0.100",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.FinishFailedTransfer(
                            0,
                            Error.IO,
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario17",
        "Modify the file during the transfer, expecct error",
        {
            "ren": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer(),
                    action.NewTransfer(
                        "172.20.0.15",
                        "/tmp/testfile-big",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.ModifyFile("/tmp/testfile-big"),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            "testfile-big",
                            Error.FILE_MODIFIED,
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "stimpy": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "172.20.0.5",
                            {
                                event.File("testfile-big", 10485760),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "testfile-big",
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, "testfile-big")),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            "testfile-big",
                            Error.BAD_TRANSFER,
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
]
