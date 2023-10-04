from drop_test import action, event
from drop_test.scenario import Scenario, ActionList
from drop_test.error import Error, ReturnCodes
from drop_test.config import FILES

from drop_test.action import PeerState

from pathlib import Path
from tempfile import gettempdir

import time
from http import HTTPStatus

# We are using the transfer slots instead of UUIDS.
# Each call to `action.NewTransfer` or the `Receive` event inserts the transfer UUID into the next slot - starting from 0

scenarios = [
    Scenario(
        "scenario1",
        "Send one file to a peer, expect it to be transferred",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "base_path": "/tmp",
                                "bytes": 10485760,
                                "bytes_sent": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.PurgeTransfers([0]),
                    action.AssertTransfers([]),
                    action.NoEvent(),
                    action.Stop(),
                    action.AssertMooseEvents(
                        [
                            """{
                            "type": "init",
                            "result": 0,
                            "app_version": "*",
                            "prod": false
                        }""",
                            """{
                            "type": "batch",
                            "transfer_id": "*",
                            "info": {
                                "mime_type_list": "unknown",
                                "extension_list": "none",
                                "file_size_list": "10240",
                                "transfer_size_kb": 10240,
                                "file_count": 1
                            },
                            "protocol_version": 5
                        }""",
                            """{
                            "type": "file",
                            "result": 0,
                            "transfer_id": "*",
                            "transfer_time": "*",
                            "direction": "upload",
                            "info": {
                                "mime_type": "unknown",
                                "extension": "none",
                                "size_kb": 10240
                            }
                        }""",
                        ]
                    ),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": false
                            }
                        ],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "bytes": 10485760,
                                "bytes_received": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/received"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.PurgeTransfers([0]),
                    action.AssertTransfers([]),
                    action.NoEvent(),
                    action.Stop(),
                    action.AssertMooseEvents(
                        [
                            """{
                            "type": "init",
                            "result": 0,
                            "app_version": "*",
                            "prod": false
                        }""",
                            """{
                            "type": "file",
                            "result": 0,
                            "transfer_id": "*",
                            "transfer_time": "*",
                            "direction": "download",
                            "info": {
                                "mime_type": "unknown",
                                "extension": "none",
                                "size_kb": 10240
                            }
                        }""",
                        ]
                    ),
                ]
            ),
        },
        tags=["moose"],
    ),
    Scenario(
        "scenario2",
        "Send two files one by one, in a different transfers. Expect it to work",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(1, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            1,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.ExpectCancel([0, 1], True),
                    action.NoEvent(),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }""",
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "base_path": "/tmp",
                                "bytes": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }""",
                        ]
                    ),
                    action.PurgeTransfersUntil(int(time.time() + 10)),
                    action.AssertTransfers([]),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        1,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(1, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            1,
                            FILES["testfile-big"].id,
                            "/tmp/received/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                            action.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": false
                            }
                        ],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/received"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }""",
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": false
                            }
                        ],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "bytes": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/received"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }""",
                        ]
                    ),
                    action.PurgeTransfersUntil(int(time.time() + 10)),
                    action.AssertTransfers([]),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["moose"],
    ),
    Scenario(
        "scenario3",
        "Send two files in parallel in two different transfers",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(latency="100ms"),
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        ),
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        ),
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                1,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-big"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["testfile-small"].id,
                            ),
                        ],
                    ),
                    action.ExpectCancel([0, 1], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        ),
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["testfile-big"].id,
                        ),
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        ),
                    ),
                    action.Download(
                        1,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.WaitRacy(
                        [
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-big"].id,
                                "/tmp/received/testfile-big",
                            ),
                            event.Start(
                                1,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileDownloaded(
                                1,
                                FILES["testfile-small"].id,
                                "/tmp/received/testfile-small",
                            ),
                        ],
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                            action.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["basic"],
    ),
    Scenario(
        "scenario4-1",
        "Send a request with one file, cancel the transfer from the sender side once it starts downloading",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.CancelTransferRequest([0]),
                    action.Wait(event.FinishTransferCanceled(0, False)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.FinishTransferCanceled(0, True)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["cancel"],
    ),
    Scenario(
        "scenario4-2",
        "Send a request with one file, cancel the transfer from the sender side before downloading",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Sleep(1),
                    action.CancelTransferRequest([0]),
                    action.Wait(event.FinishTransferCanceled(0, False)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
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
        tags=["cancel"],
    ),
    Scenario(
        "scenario4-3",
        "Send a request with one file, cancel the transfer from the receiver side once the download begins",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishTransferCanceled(0, True),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.CancelTransferRequest([0]),
                    action.Wait(
                        event.FinishTransferCanceled(0, False),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["cancel"],
    ),
    Scenario(
        "scenario4-4",
        "Send a request with one file, cancel the transfer from the receiver side before download begins",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.FinishTransferCanceled(0, True)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.Wait(
                        event.FinishTransferCanceled(0, False),
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["cancel"],
    ),
    Scenario(
        "scenario4-10",
        "Start transfer with multiple files, cancel the transfer from the receiver once the download has started",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/nested/big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["nested/big/testfile-01"].id,
                                    "big/testfile-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["nested/big/testfile-02"].id,
                                    "big/testfile-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                FILES["nested/big/testfile-01"].id,
                            ),
                            event.Start(
                                0,
                                FILES["nested/big/testfile-02"].id,
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
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["nested/big/testfile-01"].id,
                                    "big/testfile-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["nested/big/testfile-02"].id,
                                    "big/testfile-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["nested/big/testfile-01"].id,
                        "/tmp/received",
                    ),
                    action.Download(
                        0,
                        FILES["nested/big/testfile-02"].id,
                        "/tmp/received",
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                FILES["nested/big/testfile-01"].id,
                            ),
                            event.Start(
                                0,
                                FILES["nested/big/testfile-02"].id,
                            ),
                        ]
                    ),
                    action.CancelTransferRequest([0]),
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
        tags=["cancel"],
    ),
    Scenario(
        "scenario4-11",
        "Send a request with one file, cancel the request from the sender immediately, expect the other side not to be able to issue download calls",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.SleepMs(200),
                    action.CancelTransferRequest([0]),
                    action.Wait(event.FinishTransferCanceled(0, False)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.FinishTransferCanceled(0, True)),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-big"].id, Error.BAD_TRANSFER
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["cancel"],
    ),
    Scenario(
        "scenario4-12",
        "Send a request with one file to a peer that's offline. Cancel the transfer from the sender side immediately. Expect no events on the receiver once it comes online",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.Wait(event.FinishTransferCanceled(0, False)),
                    action.Sleep(4),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(1),
                    action.Start("DROP_PEER_STIMPY"),
                    action.NoEvent(4),
                    action.AssertTransfers([], 0),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "cancel"],
    ),
    Scenario(
        "scenario5-1",
        "Try to send file to an offline peer, expect silence",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList([action.Sleep(10), action.NoEvent()]),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario5-2",
        "Try to send file to an offline peer who later comes online and receives the transfer but nothing is done",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Sleep(8),
                    action.NoEvent(),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(10),
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.NoEvent(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario6",
        "Send nested directory, expect it to be transferred fully",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/deep"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "deep/path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "deep/path/file2.ext2",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file3.ext3"].id,
                                    "deep/another-path/file3.ext3",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file4.ext4"].id,
                                    "deep/another-path/file4.ext4",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/path/file1.ext1"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["deep/path/file1.ext1"].id,
                        ),
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/path/file2.ext2"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["deep/path/file2.ext2"].id,
                        ),
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/another-path/file3.ext3"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["deep/another-path/file3.ext3"].id,
                        ),
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/another-path/file4.ext4"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["deep/another-path/file4.ext4"].id,
                        ),
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "deep/path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "deep/path/file2.ext2",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file3.ext3"].id,
                                    "deep/another-path/file3.ext3",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file4.ext4"].id,
                                    "deep/another-path/file4.ext4",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["deep/path/file1.ext1"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/path/file1.ext1"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["deep/path/file1.ext1"].id,
                            "/tmp/received/deep/path/file1.ext1",
                        ),
                    ),
                    action.Download(
                        0,
                        FILES["deep/path/file2.ext2"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/path/file2.ext2"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["deep/path/file2.ext2"].id,
                            "/tmp/received/deep/path/file2.ext2",
                        ),
                    ),
                    action.Download(
                        0,
                        FILES["deep/another-path/file3.ext3"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/another-path/file3.ext3"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["deep/another-path/file3.ext3"].id,
                            "/tmp/received/deep/another-path/file3.ext3",
                        ),
                    ),
                    action.Download(
                        0,
                        FILES["deep/another-path/file4.ext4"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["deep/another-path/file4.ext4"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["deep/another-path/file4.ext4"].id,
                            "/tmp/received/deep/another-path/file4.ext4",
                        ),
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/deep/path/file1.ext1", 1048576),
                            action.File("/tmp/received/deep/path/file2.ext2", 1048576),
                            action.File(
                                "/tmp/received/deep/another-path/file3.ext3", 1048576
                            ),
                            action.File(
                                "/tmp/received/deep/another-path/file4.ext4", 1048576
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["dirs"],
    ),
    Scenario(
        "scenario7",
        "Send one file to another peer. Pre-open the file and pass a descriptor",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransferWithFD(
                        "DROP_PEER_STIMPY",
                        "/tmp/testfile-small",
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(0, "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI"),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0, "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI"
                        ),
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        ),
                    ),
                    action.Download(
                        0,
                        "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(0, "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI"),
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                            "/tmp/received/testfile-small",
                        ),
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["android"],
    ),
    Scenario(
        "scenario8-1",
        "Send two identical files one by one within different transfers, expect no overwrites to happen",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(0, FILES["testfile-small"].id)
                    ),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY", ["/tmp/duplicate/testfile-small"]
                    ),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES["duplicate/testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(1, FILES["duplicate/testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            1, FILES["duplicate/testfile-small"].id
                        )
                    ),
                    action.ExpectCancel([0, 1], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["duplicate/testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        1,
                        FILES["duplicate/testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(1, FILES["duplicate/testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            1,
                            FILES["duplicate/testfile-small"].id,
                            "/tmp/received/testfile-small(1)",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                            action.File("/tmp/received/testfile-small(1)", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["rename"],
    ),
    Scenario(
        "scenario8-2",
        "Send two identical files with complicated extensions one by one, expect appending (1), no rename or other weird stuff",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        ["/tmp/testfile.small.with.complicated.extension"],
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES[
                                        "testfile.small.with.complicated.extension"
                                    ].id,
                                    "testfile.small.with.complicated.extension",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            FILES["testfile.small.with.complicated.extension"].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile.small.with.complicated.extension"].id,
                        )
                    ),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        ["/tmp/duplicate/testfile.small.with.complicated.extension"],
                    ),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES[
                                        "duplicate/testfile.small.with.complicated.extension"
                                    ].id,
                                    "testfile.small.with.complicated.extension",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(
                            1,
                            FILES[
                                "duplicate/testfile.small.with.complicated.extension"
                            ].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            1,
                            FILES[
                                "duplicate/testfile.small.with.complicated.extension"
                            ].id,
                        )
                    ),
                    action.ExpectCancel([0, 1], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES[
                                        "testfile.small.with.complicated.extension"
                                    ].id,
                                    "testfile.small.with.complicated.extension",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile.small.with.complicated.extension"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            0, FILES["testfile.small.with.complicated.extension"].id
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile.small.with.complicated.extension"].id,
                            "/tmp/received/testfile.small.with.complicated.extension",
                        )
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES[
                                        "duplicate/testfile.small.with.complicated.extension"
                                    ].id,
                                    "testfile.small.with.complicated.extension",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        1,
                        FILES["duplicate/testfile.small.with.complicated.extension"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.Start(
                            1,
                            FILES[
                                "duplicate/testfile.small.with.complicated.extension"
                            ].id,
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            1,
                            FILES[
                                "duplicate/testfile.small.with.complicated.extension"
                            ].id,
                            "/tmp/received/testfile.small.with.complicated(1).extension",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File(
                                "/tmp/received/testfile.small.with.complicated.extension",
                                1048576,
                            ),
                            action.File(
                                "/tmp/received/testfile.small.with.complicated(1).extension",
                                1048576,
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["rename"],
    ),
    Scenario(
        "scenario11-1",
        "Send a bunch of file simultaneously and see if libdrop freezes",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    # fmt: off
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(0, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(1, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(2, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(3, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(4, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(5, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(6, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(7, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(8, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-bulk-01"]),
                    action.Wait(event.Queued(9, { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), })),
                    # fmt: on
                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["testfile-bulk-01"].id),
                            event.Start(1, FILES["testfile-bulk-01"].id),
                            event.Start(2, FILES["testfile-bulk-01"].id),
                            event.Start(3, FILES["testfile-bulk-01"].id),
                            event.Start(4, FILES["testfile-bulk-01"].id),
                            event.Start(5, FILES["testfile-bulk-01"].id),
                            event.Start(6, FILES["testfile-bulk-01"].id),
                            event.Start(7, FILES["testfile-bulk-01"].id),
                            event.Start(8, FILES["testfile-bulk-01"].id),
                            event.Start(9, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(0, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(1, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(2, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(3, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(4, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(5, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(6, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(7, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(8, FILES["testfile-bulk-01"].id),
                            event.FinishFileUploaded(9, FILES["testfile-bulk-01"].id),
                        ]
                    ),
                    # fmt: on
                    action.ExpectCancel([0, 1, 2, 3, 4, 5, 6, 7, 8, 9], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Receive(0, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(1, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(2, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(3, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(4, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(5, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(6, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(7, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(8, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                            event.Receive(9, "DROP_PEER_REN", { event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760), }),
                        ]
                    ),
                    # fmt: on
                    # fmt: off
                    action.Download(0, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/0"),
                    action.Download(1, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/1"),
                    action.Download(2, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/2"),
                    action.Download(3, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/3"),
                    action.Download(4, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/4"),
                    action.Download(5, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/5"),
                    action.Download(6, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/6"),
                    action.Download(7, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/7"),
                    action.Download(8, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/8"),
                    action.Download(9, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/9"),
                    # fmt: on
                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["testfile-bulk-01"].id),
                            event.Start(1, FILES["testfile-bulk-01"].id),
                            event.Start(2, FILES["testfile-bulk-01"].id),
                            event.Start(3, FILES["testfile-bulk-01"].id),
                            event.Start(4, FILES["testfile-bulk-01"].id),
                            event.Start(5, FILES["testfile-bulk-01"].id),
                            event.Start(6, FILES["testfile-bulk-01"].id),
                            event.Start(7, FILES["testfile-bulk-01"].id),
                            event.Start(8, FILES["testfile-bulk-01"].id),
                            event.Start(9, FILES["testfile-bulk-01"].id),
                            event.FinishFileDownloaded(0, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/0/testfile-bulk-01"),
                            event.FinishFileDownloaded(1, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/1/testfile-bulk-01"),
                            event.FinishFileDownloaded(2, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/2/testfile-bulk-01"),
                            event.FinishFileDownloaded(3, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/3/testfile-bulk-01"),
                            event.FinishFileDownloaded(4, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/4/testfile-bulk-01"),
                            event.FinishFileDownloaded(5, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/5/testfile-bulk-01"),
                            event.FinishFileDownloaded(6, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/6/testfile-bulk-01"),
                            event.FinishFileDownloaded(7, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/7/testfile-bulk-01"),
                            event.FinishFileDownloaded(8, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/8/testfile-bulk-01"),
                            event.FinishFileDownloaded(9, FILES["testfile-bulk-01"].id, "/tmp/received/11-1/9/testfile-bulk-01"),
                        ]
                    ),
                    # fmt: on
                    action.CancelTransferRequest([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]),
                    action.ExpectCancel([0, 1, 2, 3, 4, 5, 6, 7, 8, 9], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario11-2",
        "Initate a buch of transfers. Wait for some of then to start and then stop the sender. Expect immediate cancelation",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.Repeated(
                        [
                            action.NewTransfer(
                                "DROP_PEER_STIMPY",
                                [
                                    "/tmp/testfile-bulk-01",
                                    "/tmp/testfile-bulk-02",
                                    "/tmp/testfile-bulk-03",
                                    "/tmp/testfile-bulk-04",
                                ],
                            )
                        ],
                        4,
                    ),
                    # fmt: off
                    action.WaitRacy([
                        event.Queued(0, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                        event.Queued(1, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                        event.Queued(2, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                        event.Queued(3, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                    ]),
                    # fmt: on
                    # fmt: off
                    # Wait only for some of the files, let the rest requests arrive when we're in the stop() call
                    action.Repeated([action.WaitForOneOf([
                        event.Start(0, FILES["testfile-bulk-01"].id),
                        event.Start(0, FILES["testfile-bulk-02"].id),
                        event.Start(0, FILES["testfile-bulk-03"].id),
                        event.Start(0, FILES["testfile-bulk-04"].id),
                        event.Start(1, FILES["testfile-bulk-01"].id),
                        event.Start(1, FILES["testfile-bulk-02"].id),
                        event.Start(1, FILES["testfile-bulk-03"].id),
                        event.Start(1, FILES["testfile-bulk-04"].id),
                        event.Start(2, FILES["testfile-bulk-01"].id),
                        event.Start(2, FILES["testfile-bulk-02"].id),
                        event.Start(2, FILES["testfile-bulk-03"].id),
                        event.Start(2, FILES["testfile-bulk-04"].id),
                        event.Start(3, FILES["testfile-bulk-01"].id),
                        event.Start(3, FILES["testfile-bulk-02"].id),
                        event.Start(3, FILES["testfile-bulk-03"].id),
                        event.Start(3, FILES["testfile-bulk-04"].id),
                    ])], 4),
                    # fmt: on
                    action.EnsureTakesNoLonger(action.Stop(), seconds=1),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Receive(0, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                            event.Receive(1, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                            event.Receive(2, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                            event.Receive(3, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                        ]
                    ),
                    # fmt: on
                    # fmt: off
                    action.Download(0, FILES["testfile-bulk-01"].id, "/tmp/received/11-2"),
                    action.Download(0, FILES["testfile-bulk-02"].id, "/tmp/received/11-2"),
                    action.Download(0, FILES["testfile-bulk-03"].id, "/tmp/received/11-2"),
                    action.Download(0, FILES["testfile-bulk-04"].id, "/tmp/received/11-2"),
                    action.Download(1, FILES["testfile-bulk-01"].id, "/tmp/received/11-2"),
                    action.Download(1, FILES["testfile-bulk-02"].id, "/tmp/received/11-2"),
                    action.Download(1, FILES["testfile-bulk-03"].id, "/tmp/received/11-2"),
                    action.Download(1, FILES["testfile-bulk-04"].id, "/tmp/received/11-2"),
                    action.Download(2, FILES["testfile-bulk-01"].id, "/tmp/received/11-2"),
                    action.Download(2, FILES["testfile-bulk-02"].id, "/tmp/received/11-2"),
                    action.Download(2, FILES["testfile-bulk-03"].id, "/tmp/received/11-2"),
                    action.Download(2, FILES["testfile-bulk-04"].id, "/tmp/received/11-2"),
                    action.Download(3, FILES["testfile-bulk-01"].id, "/tmp/received/11-2"),
                    action.Download(3, FILES["testfile-bulk-02"].id, "/tmp/received/11-2"),
                    action.Download(3, FILES["testfile-bulk-03"].id, "/tmp/received/11-2"),
                    action.Download(3, FILES["testfile-bulk-04"].id, "/tmp/received/11-2"),
                    # fmt: on
                    # fmt: off
                    action.Repeated([action.WaitForOneOf([
                        event.Start(0, FILES["testfile-bulk-01"].id),
                        event.Start(0, FILES["testfile-bulk-02"].id),
                        event.Start(0, FILES["testfile-bulk-03"].id),
                        event.Start(0, FILES["testfile-bulk-04"].id),
                        event.Start(1, FILES["testfile-bulk-01"].id),
                        event.Start(1, FILES["testfile-bulk-02"].id),
                        event.Start(1, FILES["testfile-bulk-03"].id),
                        event.Start(1, FILES["testfile-bulk-04"].id),
                        event.Start(2, FILES["testfile-bulk-01"].id),
                        event.Start(2, FILES["testfile-bulk-02"].id),
                        event.Start(2, FILES["testfile-bulk-03"].id),
                        event.Start(2, FILES["testfile-bulk-04"].id),
                        event.Start(3, FILES["testfile-bulk-01"].id),
                        event.Start(3, FILES["testfile-bulk-02"].id),
                        event.Start(3, FILES["testfile-bulk-03"].id),
                        event.Start(3, FILES["testfile-bulk-04"].id),
                        event.Paused(0, FILES["testfile-bulk-01"].id),
                        event.Paused(0, FILES["testfile-bulk-02"].id),
                        event.Paused(0, FILES["testfile-bulk-03"].id),
                        event.Paused(0, FILES["testfile-bulk-04"].id),
                        event.Paused(1, FILES["testfile-bulk-01"].id),
                        event.Paused(1, FILES["testfile-bulk-02"].id),
                        event.Paused(1, FILES["testfile-bulk-03"].id),
                        event.Paused(1, FILES["testfile-bulk-04"].id),
                        event.Paused(2, FILES["testfile-bulk-01"].id),
                        event.Paused(2, FILES["testfile-bulk-02"].id),
                        event.Paused(2, FILES["testfile-bulk-03"].id),
                        event.Paused(2, FILES["testfile-bulk-04"].id),
                        event.Paused(3, FILES["testfile-bulk-01"].id),
                        event.Paused(3, FILES["testfile-bulk-02"].id),
                        event.Paused(3, FILES["testfile-bulk-03"].id),
                        event.Paused(3, FILES["testfile-bulk-04"].id),
                    ])], 10),
                    # fmt: on
                    action.EnsureTakesNoLonger(action.Stop(), seconds=1),
                ]
            ),
        },
    ),
    Scenario(
        "scenario11-3",
        "Initate a buch of transfers. Wait for some of then to start and then stop the receiver. Expect immediate cancelation",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.Repeated(
                        [
                            action.NewTransfer(
                                "DROP_PEER_STIMPY",
                                [
                                    "/tmp/testfile-bulk-01",
                                    "/tmp/testfile-bulk-02",
                                    "/tmp/testfile-bulk-03",
                                    "/tmp/testfile-bulk-04",
                                ],
                            )
                        ],
                        4,
                    ),
                    # fmt: off
                    action.WaitRacy([
                        event.Queued(0, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                        event.Queued(1, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                        event.Queued(2, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                        event.Queued(3, {
                            event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                            event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                            event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                            event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                        }),
                    ]),
                    # fmt: on
                    # fmt: off
                    # Wait only for some of the files, let the rest requests arrive when we're in the stop() call
                    action.Repeated([action.WaitForOneOf([
                        event.Start(0, FILES["testfile-bulk-01"].id),
                        event.Start(0, FILES["testfile-bulk-02"].id),
                        event.Start(0, FILES["testfile-bulk-03"].id),
                        event.Start(0, FILES["testfile-bulk-04"].id),
                        event.Start(1, FILES["testfile-bulk-01"].id),
                        event.Start(1, FILES["testfile-bulk-02"].id),
                        event.Start(1, FILES["testfile-bulk-03"].id),
                        event.Start(1, FILES["testfile-bulk-04"].id),
                        event.Start(2, FILES["testfile-bulk-01"].id),
                        event.Start(2, FILES["testfile-bulk-02"].id),
                        event.Start(2, FILES["testfile-bulk-03"].id),
                        event.Start(2, FILES["testfile-bulk-04"].id),
                        event.Start(3, FILES["testfile-bulk-01"].id),
                        event.Start(3, FILES["testfile-bulk-02"].id),
                        event.Start(3, FILES["testfile-bulk-03"].id),
                        event.Start(3, FILES["testfile-bulk-04"].id),
                        event.Paused(0, FILES["testfile-bulk-01"].id),
                        event.Paused(0, FILES["testfile-bulk-02"].id),
                        event.Paused(0, FILES["testfile-bulk-03"].id),
                        event.Paused(0, FILES["testfile-bulk-04"].id),
                        event.Paused(1, FILES["testfile-bulk-01"].id),
                        event.Paused(1, FILES["testfile-bulk-02"].id),
                        event.Paused(1, FILES["testfile-bulk-03"].id),
                        event.Paused(1, FILES["testfile-bulk-04"].id),
                        event.Paused(2, FILES["testfile-bulk-01"].id),
                        event.Paused(2, FILES["testfile-bulk-02"].id),
                        event.Paused(2, FILES["testfile-bulk-03"].id),
                        event.Paused(2, FILES["testfile-bulk-04"].id),
                        event.Paused(3, FILES["testfile-bulk-01"].id),
                        event.Paused(3, FILES["testfile-bulk-02"].id),
                        event.Paused(3, FILES["testfile-bulk-03"].id),
                        event.Paused(3, FILES["testfile-bulk-04"].id),
                    ])], 10),
                    # fmt: on
                    action.EnsureTakesNoLonger(action.Stop(), seconds=1),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    # fmt: off
                    action.WaitRacy(
                        [
                            event.Receive(0, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                            event.Receive(1, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                            event.Receive(2, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                            event.Receive(3, "DROP_PEER_REN", { 
                                event.File(FILES["testfile-bulk-01"].id, "testfile-bulk-01", 10485760),
                                event.File(FILES["testfile-bulk-02"].id, "testfile-bulk-02", 10485760),
                                event.File(FILES["testfile-bulk-03"].id, "testfile-bulk-03", 10485760),
                                event.File(FILES["testfile-bulk-04"].id, "testfile-bulk-04", 10485760),
                            }),
                        ]
                    ),
                    # fmt: on
                    # fmt: off
                    action.Download(0, FILES["testfile-bulk-01"].id, "/tmp/received/11-3"),
                    action.Download(0, FILES["testfile-bulk-02"].id, "/tmp/received/11-3"),
                    action.Download(0, FILES["testfile-bulk-03"].id, "/tmp/received/11-3"),
                    action.Download(0, FILES["testfile-bulk-04"].id, "/tmp/received/11-3"),
                    action.Download(1, FILES["testfile-bulk-01"].id, "/tmp/received/11-3"),
                    action.Download(1, FILES["testfile-bulk-02"].id, "/tmp/received/11-3"),
                    action.Download(1, FILES["testfile-bulk-03"].id, "/tmp/received/11-3"),
                    action.Download(1, FILES["testfile-bulk-04"].id, "/tmp/received/11-3"),
                    action.Download(2, FILES["testfile-bulk-01"].id, "/tmp/received/11-3"),
                    action.Download(2, FILES["testfile-bulk-02"].id, "/tmp/received/11-3"),
                    action.Download(2, FILES["testfile-bulk-03"].id, "/tmp/received/11-3"),
                    action.Download(2, FILES["testfile-bulk-04"].id, "/tmp/received/11-3"),
                    action.Download(3, FILES["testfile-bulk-01"].id, "/tmp/received/11-3"),
                    action.Download(3, FILES["testfile-bulk-02"].id, "/tmp/received/11-3"),
                    action.Download(3, FILES["testfile-bulk-03"].id, "/tmp/received/11-3"),
                    action.Download(3, FILES["testfile-bulk-04"].id, "/tmp/received/11-3"),
                    # fmt: on
                    # fmt: off
                    action.Repeated([action.WaitForOneOf([
                        event.Start(0, FILES["testfile-bulk-01"].id),
                        event.Start(0, FILES["testfile-bulk-02"].id),
                        event.Start(0, FILES["testfile-bulk-03"].id),
                        event.Start(0, FILES["testfile-bulk-04"].id),
                        event.Start(1, FILES["testfile-bulk-01"].id),
                        event.Start(1, FILES["testfile-bulk-02"].id),
                        event.Start(1, FILES["testfile-bulk-03"].id),
                        event.Start(1, FILES["testfile-bulk-04"].id),
                        event.Start(2, FILES["testfile-bulk-01"].id),
                        event.Start(2, FILES["testfile-bulk-02"].id),
                        event.Start(2, FILES["testfile-bulk-03"].id),
                        event.Start(2, FILES["testfile-bulk-04"].id),
                        event.Start(3, FILES["testfile-bulk-01"].id),
                        event.Start(3, FILES["testfile-bulk-02"].id),
                        event.Start(3, FILES["testfile-bulk-03"].id),
                        event.Start(3, FILES["testfile-bulk-04"].id),
                        event.Paused(0, FILES["testfile-bulk-01"].id),
                        event.Paused(0, FILES["testfile-bulk-02"].id),
                        event.Paused(0, FILES["testfile-bulk-03"].id),
                        event.Paused(0, FILES["testfile-bulk-04"].id),
                        event.Paused(1, FILES["testfile-bulk-01"].id),
                        event.Paused(1, FILES["testfile-bulk-02"].id),
                        event.Paused(1, FILES["testfile-bulk-03"].id),
                        event.Paused(1, FILES["testfile-bulk-04"].id),
                        event.Paused(2, FILES["testfile-bulk-01"].id),
                        event.Paused(2, FILES["testfile-bulk-02"].id),
                        event.Paused(2, FILES["testfile-bulk-03"].id),
                        event.Paused(2, FILES["testfile-bulk-04"].id),
                        event.Paused(3, FILES["testfile-bulk-01"].id),
                        event.Paused(3, FILES["testfile-bulk-02"].id),
                        event.Paused(3, FILES["testfile-bulk-03"].id),
                        event.Paused(3, FILES["testfile-bulk-04"].id),
                    ])], 4),
                    # fmt: on
                    action.EnsureTakesNoLonger(action.Stop(), seconds=1),
                ]
            ),
        },
    ),
    Scenario(
        "scenario12-1",
        "Transfer file to two clients simultaneously",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransferWithFD(
                        "DROP_PEER_STIMPY",
                        "/tmp/testfile-big",
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_GEORGE"),
                    action.NewTransferWithFD(
                        "DROP_PEER_GEORGE",
                        "/tmp/testfile-big",
                    ),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File(
                                        "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                                        "testfile-big",
                                        10485760,
                                    ),
                                },
                            ),
                            event.Queued(
                                1,
                                {
                                    event.File(
                                        "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                                        "testfile-big",
                                        10485760,
                                    ),
                                },
                            ),
                            event.Start(
                                1, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw"
                            ),
                            event.Start(
                                0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw"
                            ),
                            event.FinishFileUploaded(
                                1,
                                "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                            ),
                            event.FinishFileUploaded(
                                0,
                                "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                            ),
                            event.FinishTransferCanceled(0, True),
                            event.FinishTransferCanceled(1, True),
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                        "/tmp/received/stimpy",
                    ),
                    action.Wait(
                        event.Start(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                            "/tmp/received/stimpy/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/stimpy/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_GEORGE": ActionList(
                [
                    action.Start("DROP_PEER_GEORGE"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                        "/tmp/received/george",
                    ),
                    action.Wait(
                        event.Start(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                            "/tmp/received/george/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/george/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
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
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransferWithFD(
                        "DROP_PEER_STIMPY", "/tmp/testfile-small", cached=True
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_GEORGE"),
                    action.NewTransferWithFD(
                        "DROP_PEER_GEORGE", "/tmp/testfile-small", cached=True
                    ),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File(
                                        "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                                        "testfile-small",
                                        1024 * 1024,
                                    ),
                                },
                            ),
                            event.Queued(
                                1,
                                {
                                    event.File(
                                        "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                                        "testfile-small",
                                        1024 * 1024,
                                    ),
                                },
                            ),
                        ]
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0, "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI"
                            ),
                            event.Start(
                                1, "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI"
                            ),
                            event.FinishFileUploaded(
                                0,
                                "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                            ),
                            event.FinishFileUploaded(
                                1,
                                "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                            ),
                            event.FinishTransferCanceled(0, True),
                            event.FinishTransferCanceled(1, True),
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                                    "testfile-small",
                                    1024 * 1024,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                        "/tmp/received/stimpy",
                    ),
                    action.Wait(
                        event.Start(0, "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI")
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                            "/tmp/received/stimpy/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File(
                                "/tmp/received/stimpy/testfile-small", 1024 * 1024
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_GEORGE": ActionList(
                [
                    action.Start("DROP_PEER_GEORGE"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                                    "testfile-small",
                                    1024 * 1024,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                        "/tmp/received/george",
                    ),
                    action.Wait(
                        event.Start(0, "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI")
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "btveSO3-H7_lCgrUDAdTHFyY8oxDGed4j8VWaaQLnTI",
                            "/tmp/received/george/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File(
                                "/tmp/received/george/testfile-small", 1024 * 1024
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["android"],
    ),
    Scenario(
        "scenario13-1",
        "Transfer file with the same name as symlink in destination directory, expect appending `(1)` suffix",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
                            },
                        ),
                    ),
                    action.Wait(
                        event.Start(0, FILES["testfile-small"].id),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        ),
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received/symtest-files",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/symtest-files/testfile-small(1)",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File(
                                "/tmp/received/symtest-files/testfile-small(1)",
                                1 * 1024 * 1024,
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
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
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
                            },
                        ),
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received/symtest-dir",
                    ),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            FILES["testfile-small"].id,
                            Error.BAD_PATH,
                        )
                    ),
                    action.CheckFileDoesNotExist(
                        [
                            "/tmp/symtest-dir/testfile-small",
                        ]
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
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
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
                            },
                        ),
                    ),
                    action.Wait(
                        event.Start(0, FILES["testfile-small"].id),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        ),
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
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
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File(
                                "/tmp/received/testfile-small",
                                1 * 1024 * 1024,
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
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
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransferFails(
                        "DROP_PEER_STIMPY", "/tmp/testfile-small-xd"
                    ),
                    action.NoEvent(duration=2),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
                            },
                        ),
                    ),
                    action.Wait(
                        event.Start(0, FILES["testfile-small"].id),
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        ),
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1 * 1024 * 1024,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File(
                                "/tmp/received/testfile-small",
                                1 * 1024 * 1024,
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario15-1",
        "Repeated file download within single transfer. Expect file is already download error",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_FINISHED
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario15-2",
        "Send nested directory twice, expect (1) be added",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/deep/path"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "path/file2.ext2",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                FILES["deep/path/file1.ext1"].id,
                            ),
                            event.Start(
                                0,
                                FILES["deep/path/file2.ext2"].id,
                            ),
                            event.FinishFileUploaded(
                                0,
                                FILES["deep/path/file1.ext1"].id,
                            ),
                            event.FinishFileUploaded(
                                0,
                                FILES["deep/path/file2.ext2"].id,
                            ),
                        ]
                    ),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/deep/path"]),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "path/file2.ext2",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                1,
                                FILES["deep/path/file1.ext1"].id,
                            ),
                            event.Start(
                                1,
                                FILES["deep/path/file2.ext2"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["deep/path/file1.ext1"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["deep/path/file2.ext2"].id,
                            ),
                        ]
                    ),
                    action.ExpectCancel([0, 1], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "path/file2.ext2",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["deep/path/file1.ext1"].id,
                        "/tmp/received",
                    ),
                    action.Download(
                        0,
                        FILES["deep/path/file2.ext2"].id,
                        "/tmp/received",
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                FILES["deep/path/file1.ext1"].id,
                            ),
                            event.Start(
                                0,
                                FILES["deep/path/file2.ext2"].id,
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["deep/path/file1.ext1"].id,
                                "/tmp/received/path/file1.ext1",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["deep/path/file2.ext2"].id,
                                "/tmp/received/path/file2.ext2",
                            ),
                        ]
                    ),
                    action.Wait(
                        event.Receive(
                            1,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "path/file2.ext2",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        1,
                        FILES["deep/path/file1.ext1"].id,
                        "/tmp/received",
                    ),
                    action.Download(
                        1,
                        FILES["deep/path/file2.ext2"].id,
                        "/tmp/received",
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                1,
                                FILES["deep/path/file1.ext1"].id,
                            ),
                            event.Start(
                                1,
                                FILES["deep/path/file2.ext2"].id,
                            ),
                            event.FinishFileDownloaded(
                                1,
                                FILES["deep/path/file1.ext1"].id,
                                "/tmp/received/path(1)/file1.ext1",
                            ),
                            event.FinishFileDownloaded(
                                1,
                                FILES["deep/path/file2.ext2"].id,
                                "/tmp/received/path(1)/file2.ext2",
                            ),
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/path/file1.ext1", 1048576),
                            action.File("/tmp/received/path/file2.ext2", 1048576),
                            action.File("/tmp/received/path(1)/file1.ext1", 1048576),
                            action.File("/tmp/received/path(1)/file2.ext2", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario15-3",
        "Send two different directories with the same name, their content should not being merged",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY", ["/tmp/name", "/tmp/different/name"]
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["name/file-01"].id,
                                    "name/file-01",
                                    1048576,
                                ),
                                event.File(
                                    FILES["different/name/file-02"].id,
                                    "name(1)/file-02",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                FILES["name/file-01"].id,
                            ),
                            event.Start(
                                0,
                                FILES["different/name/file-02"].id,
                            ),
                            event.FinishFileUploaded(
                                0,
                                FILES["name/file-01"].id,
                            ),
                            event.FinishFileUploaded(
                                0,
                                FILES["different/name/file-02"].id,
                            ),
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["name/file-01"].id,
                                    "name/file-01",
                                    1048576,
                                ),
                                event.File(
                                    FILES["different/name/file-02"].id,
                                    "name(1)/file-02",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["name/file-01"].id,
                        "/tmp/received/15-3",
                    ),
                    action.Download(
                        0,
                        FILES["different/name/file-02"].id,
                        "/tmp/received/15-3",
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0,
                                FILES["name/file-01"].id,
                            ),
                            event.Start(
                                0,
                                FILES["different/name/file-02"].id,
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["name/file-01"].id,
                                "/tmp/received/15-3/name/file-01",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["different/name/file-02"].id,
                                "/tmp/received/15-3/name(1)/file-02",
                            ),
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/15-3/name/file-01", 1048576),
                            action.File("/tmp/received/15-3/name(1)/file-02", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario17",
        "Modify the file during the transfer, expect error",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.ModifyFile("/tmp/testfile-big"),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            FILES["testfile-big"].id,
                            Error.FILE_MODIFIED,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "base_path": "/tmp",
                                "bytes": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "failed",
                                        "status_code": 28
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                    action.AssertMooseEvents(
                        [
                            """{
                            "type": "init",
                            "result": 0,
                            "app_version": "*",
                            "prod": false
                        }""",
                            """{
                            "type": "batch",
                            "transfer_id": "*",
                            "info": {
                                "mime_type_list": "unknown",
                                "extension_list": "none",
                                "file_size_list": "10240",
                                "transfer_size_kb": 10240,
                                "file_count": 1
                            },
                            "protocol_version": 5
                        }""",
                            """{
                            "type": "file",
                            "result": 28,
                            "transfer_id": "*",
                            "transfer_time": "*",
                            "direction": "upload",
                            "info": {
                                "mime_type": "unknown",
                                "extension": "none",
                                "size_kb": 10240
                            }
                        }""",
                        ]
                    ),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            FILES["testfile-big"].id,
                            Error.BAD_TRANSFER_STATE,
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": false
                            }
                        ],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "bytes": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/received"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "failed",
                                        "status_code": 8
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                    action.AssertMooseEvents(
                        [
                            """{
                            "type": "init",
                            "result": 0,
                            "app_version": "*",
                            "prod": false
                        }""",
                            """{
                            "type": "file",
                            "result": 8,
                            "transfer_id": "*",
                            "transfer_time": "*",
                            "direction": "download",
                            "info": {
                                "mime_type": "unknown",
                                "extension": "none",
                                "size_kb": 10240
                            }
                        }""",
                        ]
                    ),
                ]
            ),
        },
    ),
    Scenario(
        "scenario17-1",
        "Try to send an empty directory, expect error",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransferFails("DROP_PEER_STIMPY", "/tmp/empty-dir"),
                    action.NoEvent(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.NoEvent(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario18",
        "Check if temporary file gets deleted after successful transfer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received/18",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/18/testfile-small",
                        )
                    ),
                    action.CompareTrees(
                        Path(gettempdir()) / "received" / "18",
                        [action.File("testfile-small", 1048576)],
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    # Android team reported that sending file with too long name multiple times produces different results
    Scenario(
        "scenario19-1",
        "Send file with too long name to two peers twice, expect it to fail each time",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        [
                            "/tmp/thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ],
                    ),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        [
                            "/tmp/thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ],
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_GEORGE"),
                    action.NewTransfer(
                        "DROP_PEER_GEORGE",
                        [
                            "/tmp/thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ],
                    ),
                    action.NewTransfer(
                        "DROP_PEER_GEORGE",
                        [
                            "/tmp/thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ],
                    ),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Queued(
                                1,
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Queued(
                                2,
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Queued(
                                3,
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                            event.FinishTransferCanceled(0, True),
                            event.FinishTransferCanceled(1, True),
                            event.FinishTransferCanceled(2, True),
                            event.FinishTransferCanceled(3, True),
                        ]
                    ),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.WaitRacy(
                        [
                            event.Receive(
                                0,
                                "DROP_PEER_REN",
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Receive(
                                1,
                                "DROP_PEER_REN",
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                        ]
                    ),
                    action.Download(
                        0,
                        FILES[
                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ].id,
                        "/tmp/received/19-1/stimpy/0",
                    ),
                    action.Download(
                        1,
                        FILES[
                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ].id,
                        "/tmp/received/19-1/stimpy/1",
                    ),
                    action.WaitRacy(
                        [
                            event.FinishFileFailed(
                                0,
                                FILES[
                                    "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                ].id,
                                Error.FILENAME_TOO_LONG,
                            ),
                            event.FinishFileFailed(
                                1,
                                FILES[
                                    "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                ].id,
                                Error.FILENAME_TOO_LONG,
                            ),
                        ]
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_GEORGE": ActionList(
                [
                    action.Start("DROP_PEER_GEORGE"),
                    action.WaitRacy(
                        [
                            event.Receive(
                                0,
                                "DROP_PEER_REN",
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Receive(
                                1,
                                "DROP_PEER_REN",
                                {
                                    event.File(
                                        FILES[
                                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                        ].id,
                                        "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt",
                                        1048576,
                                    ),
                                },
                            ),
                        ]
                    ),
                    action.Download(
                        0,
                        FILES[
                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ].id,
                        "/tmp/received/19-1/george/0",
                    ),
                    action.Download(
                        1,
                        FILES[
                            "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                        ].id,
                        "/tmp/received/19-1/george/1",
                    ),
                    action.WaitRacy(
                        [
                            event.FinishFileFailed(
                                0,
                                FILES[
                                    "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                ].id,
                                Error.FILENAME_TOO_LONG,
                            ),
                            event.FinishFileFailed(
                                1,
                                FILES[
                                    "thisisaverylongfilenameusingonlylowercaselettersandnumbersanditcontainshugestringofnumbers01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234561234567891234567891234567890123456789012345678901234567890123456.txt"
                                ].id,
                                Error.FILENAME_TOO_LONG,
                            ),
                        ]
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    # TODO: for some reason after migration to paralllel runner this test started to fail
    # the new runner expects the files already created, but the old one created at runtime, maybe there was a difference
    Scenario(
        "scenario19-2",
        # While we do replace ASCII control chars, they are technically allowed on Linux. So we can write and run the test
        "Send a file with a ASCII control char in the name, expect it to being renamed to '_'",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    # Wait for another peer to appear
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY", ["/tmp/with-illegal-char-\x0A-"]
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["with-illegal-char-\x0A-"].id,
                                    "with-illegal-char-\x0A-",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["with-illegal-char-\x0A-"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["with-illegal-char-\x0A-"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["with-illegal-char-\x0A-"].id,
                                    "with-illegal-char-\x0A-",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["with-illegal-char-\x0A-"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["with-illegal-char-\x0A-"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["with-illegal-char-\x0A-"].id,
                            "/tmp/received/with-illegal-char-_-",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/with-illegal-char-_-", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario20",
        "Send multiple files within a single transfer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        [
                            "/tmp/testfile-small",
                            "/tmp/testfile-big",
                            "/tmp/deep",
                        ],
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "deep/path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "deep/path/file2.ext2",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file3.ext3"].id,
                                    "deep/another-path/file3.ext3",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file4.ext4"].id,
                                    "deep/another-path/file4.ext4",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["testfile-small"].id),
                            event.Start(0, FILES["testfile-big"].id),
                            event.Start(0, FILES["deep/path/file1.ext1"].id),
                            event.Start(0, FILES["deep/path/file2.ext2"].id),
                            event.Start(0, FILES["deep/another-path/file3.ext3"].id),
                            event.Start(0, FILES["deep/another-path/file4.ext4"].id),
                            event.FinishFileUploaded(0, FILES["testfile-small"].id),
                            event.FinishFileUploaded(0, FILES["testfile-big"].id),
                            event.FinishFileUploaded(
                                0, FILES["deep/path/file1.ext1"].id
                            ),
                            event.FinishFileUploaded(
                                0, FILES["deep/path/file2.ext2"].id
                            ),
                            event.FinishFileUploaded(
                                0, FILES["deep/another-path/file3.ext3"].id
                            ),
                            event.FinishFileUploaded(
                                0, FILES["deep/another-path/file4.ext4"].id
                            ),
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "deep/path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "deep/path/file2.ext2",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file3.ext3"].id,
                                    "deep/another-path/file3.ext3",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/another-path/file4.ext4"].id,
                                    "deep/another-path/file4.ext4",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(0, FILES["testfile-small"].id, "/tmp/received/20"),
                    action.Download(0, FILES["testfile-big"].id, "/tmp/received/20"),
                    action.Download(
                        0, FILES["deep/path/file1.ext1"].id, "/tmp/received/20"
                    ),
                    action.Download(
                        0, FILES["deep/path/file2.ext2"].id, "/tmp/received/20"
                    ),
                    action.Download(
                        0, FILES["deep/another-path/file3.ext3"].id, "/tmp/received/20"
                    ),
                    action.Download(
                        0, FILES["deep/another-path/file4.ext4"].id, "/tmp/received/20"
                    ),
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["testfile-small"].id),
                            event.Start(0, FILES["testfile-big"].id),
                            event.Start(0, FILES["deep/path/file1.ext1"].id),
                            event.Start(0, FILES["deep/path/file2.ext2"].id),
                            event.Start(0, FILES["deep/another-path/file3.ext3"].id),
                            event.Start(0, FILES["deep/another-path/file4.ext4"].id),
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-small"].id,
                                "/tmp/received/20/testfile-small",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-big"].id,
                                "/tmp/received/20/testfile-big",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["deep/path/file1.ext1"].id,
                                "/tmp/received/20/deep/path/file1.ext1",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["deep/path/file2.ext2"].id,
                                "/tmp/received/20/deep/path/file2.ext2",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["deep/another-path/file3.ext3"].id,
                                "/tmp/received/20/deep/another-path/file3.ext3",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["deep/another-path/file4.ext4"].id,
                                "/tmp/received/20/deep/another-path/file4.ext4",
                            ),
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/20/testfile-small", 1048576),
                            action.File("/tmp/received/20/testfile-big", 10485760),
                            action.File(
                                "/tmp/received/20/deep/path/file1.ext1", 1048576
                            ),
                            action.File(
                                "/tmp/received/20/deep/path/file2.ext2", 1048576
                            ),
                            action.File(
                                "/tmp/received/20/deep/another-path/file3.ext3", 1048576
                            ),
                            action.File(
                                "/tmp/received/20/deep/another-path/file4.ext4", 1048576
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario21-1",
        "Stop the file transfer in flight from the receiver side by going offline, then download it again. Expect to resume using the temporary file",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(latency="0.5s"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.SleepMs(300),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(event.FinishFileUploaded(0, FILES["testfile-big"].id)),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/21-1-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/21-1",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/21-1-stimpy.sqlite"
                    ),
                    action.WaitForResume(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/21-1/*.dropdl-part",
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/21-1/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/21-1/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario21-2",
        "Stop the file transfer in flight and modify the temporary file, expect discarding the temporary file",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(latency="0.5s"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/21-2-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/21-2",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    # modify the file
                    action.ModifyFile("/tmp/received/21-2/*.dropdl-part"),
                    # restart
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/21-2-stimpy.sqlite"
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/21-2/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/21-2/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario21-3",
        "Stop the directory transfer in flight, then resume transfer. Expect it to be resumed properly",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(latency="0.5s"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/nested"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["nested/big/testfile-01"].id,
                                    "nested/big/testfile-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["nested/big/testfile-02"].id,
                                    "nested/big/testfile-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["nested/big/testfile-01"].id),
                            event.Start(0, FILES["nested/big/testfile-02"].id),
                        ]
                    ),
                    action.WaitRacy(
                        [
                            event.Paused(0, FILES["nested/big/testfile-01"].id),
                            event.Paused(0, FILES["nested/big/testfile-02"].id),
                        ]
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.WaitRacy(
                        [
                            event.Start(
                                0, FILES["nested/big/testfile-01"].id, transferred=None
                            ),
                            event.Start(
                                0, FILES["nested/big/testfile-02"].id, transferred=None
                            ),
                        ]
                    ),
                    action.WaitRacy(
                        [
                            event.FinishFileUploaded(
                                0, FILES["nested/big/testfile-01"].id
                            ),
                            event.FinishFileUploaded(
                                0, FILES["nested/big/testfile-02"].id
                            ),
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/21-3-stimpy.sqlite"
                    ),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["nested/big/testfile-01"].id,
                                    "nested/big/testfile-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["nested/big/testfile-02"].id,
                                    "nested/big/testfile-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["nested/big/testfile-01"].id,
                        "/tmp/received/21-3",
                    ),
                    action.Download(
                        0,
                        FILES["nested/big/testfile-02"].id,
                        "/tmp/received/21-3",
                    ),
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["nested/big/testfile-01"].id),
                            event.Start(0, FILES["nested/big/testfile-02"].id),
                            # wait for the initial progress indicating that we start from the beginning
                            event.Progress(0, FILES["nested/big/testfile-01"].id, 0),
                            event.Progress(0, FILES["nested/big/testfile-02"].id, 0),
                            # make sure we have received something, so that we have non-empty tmp file
                            event.Progress(0, FILES["nested/big/testfile-01"].id),
                            event.Progress(0, FILES["nested/big/testfile-02"].id),
                        ]
                    ),
                    action.Stop(),
                    action.WaitRacy(
                        [
                            event.Paused(0, FILES["nested/big/testfile-01"].id),
                            event.Paused(0, FILES["nested/big/testfile-02"].id),
                        ]
                    ),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/21-3-stimpy.sqlite"
                    ),
                    action.WaitRacy(
                        [
                            event.Start(
                                0, FILES["nested/big/testfile-01"].id, transferred=None
                            ),
                            event.Start(
                                0, FILES["nested/big/testfile-02"].id, transferred=None
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["nested/big/testfile-01"].id,
                                "/tmp/received/21-3/nested/big/testfile-01",
                            ),
                            event.FinishFileDownloaded(
                                0,
                                FILES["nested/big/testfile-02"].id,
                                "/tmp/received/21-3/nested/big/testfile-02",
                            ),
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File(
                                "/tmp/received/21-3/nested/big/testfile-01", 10485760
                            ),
                            action.File(
                                "/tmp/received/21-3/nested/big/testfile-02", 10485760
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario22",
        "Send one zero sized file to a peer, expect it to be transferred",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/zero-sized-file"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["zero-sized-file"].id, "zero-sized-file", 0
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["zero-sized-file"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["zero-sized-file"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["zero-sized-file"].id, "zero-sized-file", 0
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["zero-sized-file"].id,
                        "/tmp/received/22",
                    ),
                    action.Wait(event.Start(0, FILES["zero-sized-file"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["zero-sized-file"].id,
                            "/tmp/received/22/zero-sized-file",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/22/zero-sized-file", 0),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario23-1",
        "Send two files with the same name but different fs location, expect them to transfer successfully",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        ["/tmp/testfile-small", "/tmp/duplicate/testfile-small"],
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                                event.File(
                                    FILES["duplicate/testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.Wait(event.Start(0, FILES["duplicate/testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["duplicate/testfile-small"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                                event.File(
                                    FILES["duplicate/testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received/23-1",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/23-1/testfile-small",
                        )
                    ),
                    action.Download(
                        0,
                        FILES["duplicate/testfile-small"].id,
                        "/tmp/received/23-1",
                    ),
                    action.Wait(event.Start(0, FILES["duplicate/testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["duplicate/testfile-small"].id,
                            "/tmp/received/23-1/testfile-small(1)",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/23-1/testfile-small", 1048576),
                            action.File(
                                "/tmp/received/23-1/testfile-small(1)", 1048576
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario24",
        "Download file into a readonly directory. Expect failure",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.BAD_TRANSFER_STATE
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.DropPrivileges(),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/root",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileFailed(
                            0,
                            FILES["testfile-small"].id,
                            Error.IO,
                            13,
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario25",
        "Delete temporary file during the transfer, expect it to fail",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-big"].id, Error.BAD_TRANSFER_STATE
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/25",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.DeleteFile("/tmp/received/25/*.dropdl-part"),
                    action.Wait(
                        event.FinishFileFailed(0, FILES["testfile-big"].id, Error.IO, 2)
                    ),
                    action.CompareTrees(Path("/tmp/received/25"), []),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario26-1",
        "Test if the instance can recover on database corruption",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start(
                        "DROP_PEER_REN",
                        dbpath="/tmp/db/26-1-corrupted.sqlite",
                    ),
                    action.Wait(event.RuntimeError(Error.DB_LOST)),
                    action.NoEvent(),
                    action.Stop(),
                    action.AssertMooseEvents(
                        [
                            """{
                            "type": "exception",
                            "arbitrary_value": -1,
                            "code": 11,
                            "note": "Initial DB open failed, recreating",
                            "message": "Failed to open DB file",
                            "name": "DB Error"
                        }""",
                            """{
                            "type": "init",
                            "result": 0,
                            "app_version": "*",
                            "prod": false
                        }""",
                        ]
                    ),
                ]
            ),
        },
        tags=["moose"],
    ),
    Scenario(
        "scenario26-2",
        "Provide database with insufficient permissions, expect file share and in-memory database to work, with failures being reported to moose",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.DropPrivileges(),
                    action.Start(
                        "DROP_PEER_REN",
                        dbpath="/root/no-access-db.sqlite",
                    ),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }""",
                        ]
                    ),
                    action.Stop(),
                    action.AssertMooseEvents(
                        [
                            """{
                            "type": "exception",
                            "arbitrary_value": -1,
                            "code": 11,
                            "note": "Initial DB open failed, recreating",
                            "message": "Failed to open DB file",
                            "name": "DB Error"
                        }""",
                            """{
                            "type": "exception",
                            "arbitrary_value": -1,
                            "code": 11,
                            "note": "Permission denied (os error 13)",
                            "message": "Failed to remove old DB file",
                            "name": "DB Error"
                        }""",
                            """{
                            "type": "init",
                            "result": 0,
                            "app_version": "*",
                            "prod": false
                        }""",
                            """{
                            "type": "batch",
                            "transfer_id": "*",
                            "info": {
                                "mime_type_list": "unknown",
                                "extension_list": "none",
                                "file_size_list": "1024",
                                "transfer_size_kb": 1024,
                                "file_count": 1
                            },
                            "protocol_version": 5
                        }""",
                            """{
                            "type": "file",
                            "result": 0,
                            "transfer_id": "*",
                            "transfer_time": "*",
                            "direction": "upload",
                            "info": {
                                "mime_type": "unknown",
                                "extension": "none",
                                "size_kb": 1024
                            }
                        }""",
                        ]
                    ),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received/26-2",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/26-2/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/26-2/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                ]
            ),
        },
        tags=["moose"],
    ),
    Scenario(
        "scenario27-1",
        "Reject file on sending side. Expect event on both peers",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-2",
        "Reject file on receiving side. Expect event on both peers",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-3",
        "Reject currently transmitted file on sender side. Expect event on both peers",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.RejectTransferFile(0, FILES["testfile-big"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-big"].id, False)
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(0, FILES["testfile-big"].id, "/tmp/received/27-3"),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-big"].id, True)
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-4",
        "Reject currently transmitted file on receiver side. Expect event on both peers",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.ConfigureNetwork(),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-big"].id, True)
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(0, FILES["testfile-big"].id, "/tmp/received/27-4"),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.RejectTransferFile(0, FILES["testfile-big"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-big"].id, False)
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-5",
        "Reject file on sender side, then try to download it. Expect event on both peers plus error event on the receiver side",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.Download(
                        0, FILES["testfile-small"].id, "/tmp/received/27-5"
                    ),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_REJECTED
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-6",
        "Reject file on receiver side, then try to download it. Expect event on both peers plus error event on the receiver side",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.Download(
                        0, FILES["testfile-small"].id, "/tmp/received/27-6"
                    ),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_REJECTED
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-7",
        "Reject file on sender side, then try to reject it again from both sides. Expect event on both peers plus error upon rejecting again",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.SleepMs(200),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_REJECTED
                        ),
                    ),
                    action.SleepMs(200),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_REJECTED
                        ),
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-8",
        "Reject file on receiver side, then try to reject it again from both sides. Expect event on both peers plus error upon rejecting again",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_REJECTED
                        ),
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_REJECTED
                        ),
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario27-9",
        "Download file and then try to reject it from both peers. Expect not being able to reject competed files",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(0, FILES["testfile-small"].id)
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_FINISHED
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0, FILES["testfile-small"].id, "/tmp/received/27-9"
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/27-9/testfile-small",
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-small"].id, Error.FILE_FINISHED
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject"],
    ),
    Scenario(
        "scenario28",
        "Send one file to a peer overt the IPv6 network, expect it to be transferred",
        {
            "DROP_PEER_REN6": ActionList(
                [
                    action.Start("DROP_PEER_REN6"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY6"),
                    action.NewTransfer("DROP_PEER_STIMPY6", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.PurgeTransfers([0]),
                    action.AssertTransfers([]),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY6": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY6"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN6",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received/28",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/28/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/28/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": false
                            }
                        ],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/received/28"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.PurgeTransfers([0]),
                    action.AssertTransfers([]),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["ipv6"],
    ),
    Scenario(
        "scenario29-1",
        "Send one file to a peer, stop the sender and then start back. Expect automatically restored transfer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-1-ren.sqlite"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    # start the sender again
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-1-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "base_path": "/tmp",
                                "bytes": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "paused",
                                        "bytes_sent": "*"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": "*"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/29-1",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/29-1/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/29-1/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": false
                            }
                        ],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-big",
                                "bytes": 10485760,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/received/29-1"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "paused",
                                        "bytes_received": "*"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": "*"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "moose"],
    ),
    Scenario(
        "scenario29-2",
        "Send one file to a peer, stop the receiver and then start back. Expect automatically restored transfer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    # TODO: could be nice to actually signal the peer via docker that peer is online, this would reduce flakyness
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-2-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/29-2",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    # start the receiver again
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-2-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/29-2/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/29-2/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario29-3",
        "Send three files to a peer, download one, reject one and do nothing with third one. Then stop the sender and then start back. Expect automatically restored transfer, without the rejected and stopped file",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-3-ren.sqlite"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        [
                            "/tmp/testfile-big",
                            "/tmp/testfile-bulk-01",
                            "/tmp/testfile-bulk-02",
                        ],
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-01"].id,
                                    "testfile-bulk-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-02"].id,
                                    "testfile-bulk-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-bulk-01"].id, True)
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    # start the sender again
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-3-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-01"].id,
                                    "testfile-bulk-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-02"].id,
                                    "testfile-bulk-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-bulk-01"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-bulk-01"].id, False)
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/29-3",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/29-3/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/29-3/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "reject"],
    ),
    Scenario(
        "scenario29-4",
        "Send three files to a peer. Download one, reject one and do nothing with the third one and restart the receiver. Expect automatically restored transfer and the remaining file to be downloaded with no acitvity on others",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY",
                        [
                            "/tmp/testfile-big",
                            "/tmp/testfile-bulk-01",
                            "/tmp/testfile-bulk-02",
                        ],
                    ),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-01"].id,
                                    "testfile-bulk-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-02"].id,
                                    "testfile-bulk-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-bulk-01"].id, True)
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-4-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-01"].id,
                                    "testfile-bulk-01",
                                    10485760,
                                ),
                                event.File(
                                    FILES["testfile-bulk-02"].id,
                                    "testfile-bulk-02",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-bulk-01"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-bulk-01"].id, False)
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/29-4",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    # start the receiver again
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-4-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Start(0, FILES["testfile-big"].id, transferred=None)
                    ),
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/29-4/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/29-4/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject", "offline"],
    ),
    Scenario(
        "scenario29-5",
        "Send one file FD to a peer, stop the sender and then start back. Expect automatically restored transfer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-5-ren.sqlite"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransferWithFD("DROP_PEER_STIMPY", "/tmp/testfile-big"),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.Start(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(
                        event.Progress(
                            0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw", 0
                        )
                    ),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(
                        event.Progress(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    action.Stop(),
                    action.Wait(
                        event.Paused(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    # start the sender again
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-5-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.Start(
                            0,
                            "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                            transferred=None,
                        )
                    ),
                    action.Wait(
                        event.Progress(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                        "/tmp/received/29-5",
                    ),
                    action.Wait(
                        event.Start(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    action.Wait(
                        event.Paused(0, "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw")
                    ),
                    action.Wait(
                        event.Start(
                            0,
                            "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                            transferred=None,
                        )
                    ),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            "jbKuIzVPNMpYyBXk0DGoiEFXi3HoJ3wnGrygOYgdoKw",
                            "/tmp/received/29-5/testfile-big",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/29-5/testfile-big", 10485760),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["reject", "offline"],
    ),
    Scenario(
        "scenario29-6",
        "Start file transfer to offline peer, stop the sender, start the sender and the receiver, expect the transfer to be received by the receiver",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-6-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Stop(),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-6-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.NoEvent(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(4),
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario29-7",
        "Start file transfer to offline peer, reject the file, start the sender and the receiver, expect the rejection to be signalled alongside new transfer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-7-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-big"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-big"].id, False)
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(2),
                    # at this point the transfer from the sender should have failed already
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-big"].id, True)
                    ),
                    action.Stop(),
                    action.NoEvent(),
                ]
            ),
        },
        tags=["reject", "offline"],
    ),
    Scenario(
        "scenario29-8",
        "Start file transfer to offline peer, cancel the transfer, start the sender and the receiver, expect the received to have silence",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-8-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Stop(),
                    action.Sleep(2),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-8-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(1),
                    action.Start("DROP_PEER_STIMPY"),
                    action.NoEvent(),
                    # No events, just updated database
                    action.AssertTransfers([], 0),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario29-9",
        "Complete a transfer, restart both sides, expect nothing to happen",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-9-ren.sqlite"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-9-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.NoEvent(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-9-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Stop(),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-9-stimpy.sqlite"
                    ),
                    action.NoEvent(),
                ]
            ),
        },
        tags=["basic"],
    ),
    Scenario(
        "scenario29-10",
        "Send a transfer request, wait for it to arrive, stop the sender, accept on the receiver, start the sender, expect the transfer to be resumed",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-10-ren.sqlite"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    # give it some time to arrive
                    # TODO: it would be better if we would have an explicit event for this
                    # or slightly worse - coordinate testrunners from two peers via central container about stuff which happened
                    action.Sleep(3),
                    action.Stop(),
                    action.Sleep(3),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-10-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-10-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_REN", PeerState.Offline),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario29-11",
        "Send a transfer request, wait for it to arrive, stop the sender, reject on the receiver, start the sender, expect the transfer to not be resumed",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-11-ren.sqlite"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    # give it some time to arrive
                    # TODO: it would be better if we would have an explicit event for this
                    # or slightly worse - coordinate testrunners from two peers via central container about stuff which happened
                    action.Sleep(3),
                    action.Stop(),
                    action.Sleep(3),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-11-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.NoEvent(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-11-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_REN", PeerState.Offline),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.NoEvent(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario29-12",
        "Send a transfer request, wait for it to arrive, stop the sender, cancel on the receiver, start the sender, expect the transfer to not be resumed",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-12-ren.sqlite"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    # give it some time to arrive
                    # TODO: it would be better if we would have an explicit event for this
                    # or slightly worse - coordinate testrunners from two peers via central container about stuff which happened
                    action.Sleep(3),
                    action.Stop(),
                    action.Sleep(3),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-12-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-12-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitForAnotherPeer("DROP_PEER_REN", PeerState.Offline),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario29-13",
        "Send a transfer request, start transfer and then stop the sender. Then remove transfered file and start the sender. Expect proper transfer restoration and file transfer failure",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-13-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    # wait for the initial progress indicating that we start from the beginning
                    action.Wait(event.Progress(0, FILES["testfile-big"].id, 0)),
                    # make sure we have received something, so that we have non-empty tmp file
                    action.Wait(event.Progress(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.DeleteFileFromFS("/tmp/testfile-big"),
                    # restart
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/29-13-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-big"].id, Error.IO, os_err=2
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/29-11-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0, FILES["testfile-big"].id, "/tmp/received/29-13/"
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileFailed(
                            0, FILES["testfile-big"].id, Error.BAD_TRANSFER_STATE
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario30",
        "Trigger DDoS protection. Expect some transfers to fail",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ExpectAnyError(
                        action.Parallel(
                            [
                                action.MakeHttpGetRequest(
                                    "DROP_PEER_STIMPY",
                                    "/non-existing-path",
                                    HTTPStatus.NOT_FOUND,
                                )
                            ]
                            * 1500,
                        ),
                    ),
                    action.MakeHttpGetRequest(
                        "DROP_PEER_STIMPY",
                        "/non-existing-path",
                        HTTPStatus.TOO_MANY_REQUESTS,
                    ),
                    action.Sleep(10),
                    # check if it's all good again
                    action.MakeHttpGetRequest(
                        "DROP_PEER_STIMPY", "/non-existing-path", HTTPStatus.NOT_FOUND
                    ),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Sleep(30),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario31-1",
        "Remove rejected file on sending side. Expect file not being present in the sender JSON output",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/31-1-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "rejected",
                                        "by_peer": false
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.RemoveTransferFile(0, FILES["testfile-small"].id),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": []
                    }"""
                        ]
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/31-1-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "rejected",
                                        "by_peer": true
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario31-2",
        "Remove rejected file on receiver side. Expect file not being present in the receiver JSON output",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/31-2-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, True)
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "rejected",
                                        "by_peer": true
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/31-2-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.RejectTransferFile(0, FILES["testfile-small"].id),
                    action.Wait(
                        event.FinishFileRejected(0, FILES["testfile-small"].id, False)
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "rejected",
                                        "by_peer": false
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.RemoveTransferFile(0, FILES["testfile-small"].id),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": []
                    }"""
                        ]
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario31-3",
        "Remove file from the transfer on sending side without rejecting it first. Expect API error and file present in database output",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/31-1-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectError(
                        action.RemoveTransferFile(0, FILES["testfile-small"].id),
                        Error.BAD_FILE,
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.SleepMs(200),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/31-1-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Sleep(2),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario31-4",
        "Remove file from the transfer on the receiving side without rejecting it first. Expect API error and file present in the database output",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/31-1-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Sleep(2),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/31-1-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Sleep(2),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectError(
                        action.RemoveTransferFile(0, FILES["testfile-small"].id),
                        Error.BAD_FILE,
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["database"],
    ),
    Scenario(
        "scenario31-5",
        "Remove completed file on sending side. Expect file not being present in the sender JSON output",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(0, FILES["testfile-small"].id)
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "bytes_sent": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.RemoveTransferFile(0, FILES["testfile-small"].id),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": []
                    }"""
                        ]
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(0, FILES["testfile-small"].id, "/tmp/recv/31-5"),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/recv/31-5/testfile-small",
                        )
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "bytes_received": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/recv/31-5"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed",
                                        "final_path": "/tmp/recv/31-5/testfile-small"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario31-6",
        "Remove completed file on receiving side. Expect file not being present in the sender JSON output",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(0, FILES["testfile-small"].id)
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "bytes_sent": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_sent": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(0, FILES["testfile-small"].id, "/tmp/recv/31-6"),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/recv/31-6/testfile-small",
                        )
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "bytes_received": 1048576,
                                "states": [
                                    {
                                        "created_at": "*",
                                        "state": "pending",
                                        "base_dir": "/tmp/recv/31-6"
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "started",
                                        "bytes_received": 0
                                    },
                                    {
                                        "created_at": "*",
                                        "state": "completed",
                                        "final_path": "/tmp/recv/31-6/testfile-small"
                                    }
                                ]
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.RemoveTransferFile(0, FILES["testfile-small"].id),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": []
                    }"""
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario31-7",
        "Try to remove non terminated file on both sides. Expect removal failure",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "bytes_sent": 0,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectError(
                        action.RemoveTransferFile(0, FILES["testfile-small"].id),
                        ReturnCodes.BAD_INPUT,
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "bytes_sent": 0,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.SleepMs(200),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "bytes_received": 0,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectError(
                        action.RemoveTransferFile(0, FILES["testfile-small"].id),
                        ReturnCodes.BAD_INPUT,
                    ),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "bytes_received": 0,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["moose"],
    ),
    Scenario(
        "scenario31-8",
        "Send two nested files. Check if after the reconnection the newly created directory is used instead of creating a new one",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/deep/path"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "path/file2.ext2",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["deep/path/file1.ext1"].id),
                            event.FinishFileUploaded(
                                0, FILES["deep/path/file1.ext1"].id
                            ),
                        ]
                    ),
                    action.Sleep(4),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["deep/path/file2.ext2"].id),
                            event.FinishFileUploaded(
                                0, FILES["deep/path/file2.ext2"].id
                            ),
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/31-8-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["deep/path/file1.ext1"].id,
                                    "path/file1.ext1",
                                    1048576,
                                ),
                                event.File(
                                    FILES["deep/path/file2.ext2"].id,
                                    "path/file2.ext2",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0, FILES["deep/path/file1.ext1"].id, "/tmp/received/31-8/"
                    ),
                    action.Wait(event.Start(0, FILES["deep/path/file1.ext1"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["deep/path/file1.ext1"].id,
                            "/tmp/received/31-8/path/file1.ext1",
                        )
                    ),
                    action.Stop(),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/31-8-stimpy.sqlite"
                    ),
                    action.Download(
                        0, FILES["deep/path/file2.ext2"].id, "/tmp/received/31-8/"
                    ),
                    action.Wait(event.Start(0, FILES["deep/path/file2.ext2"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["deep/path/file2.ext2"].id,
                            "/tmp/received/31-8/path/file2.ext2",
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario32",
        "Send the same file with two different transfer into the same directory. Expect no errors",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File(
                                        FILES["testfile-small"].id,
                                        "testfile-small",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Queued(
                                1,
                                {
                                    event.File(
                                        FILES["testfile-small"].id,
                                        "testfile-small",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Start(
                                0,
                                FILES["testfile-small"].id,
                            ),
                            event.Start(
                                1,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["testfile-small"].id,
                            ),
                        ],
                    ),
                    action.ExpectCancel([0, 1], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.WaitRacy(
                        [
                            event.Receive(
                                0,
                                "DROP_PEER_REN",
                                {
                                    event.File(
                                        FILES["testfile-small"].id,
                                        "testfile-small",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Receive(
                                1,
                                "DROP_PEER_REN",
                                {
                                    event.File(
                                        FILES["testfile-small"].id,
                                        "testfile-small",
                                        1048576,
                                    ),
                                },
                            ),
                        ]
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received/32/",
                    ),
                    action.Download(
                        1,
                        FILES["testfile-small"].id,
                        "/tmp/received/32/",
                    ),
                    # We cannot predict the final path of files from each transfer so we cannot wait for specific event
                    action.DrainEvents(4),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/32/testfile-small", 1048576),
                            action.File("/tmp/received/32/testfile-small(1)", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario33",
        "Create transfer by specifying relative file path. Expect file to be resolved and stored in the DB",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer(
                        "DROP_PEER_STIMPY", ["../../tmp/testfile-small"]
                    ),  # CWD is /src
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": false
                            }
                        ],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.Stop(),
                    action.AssertMooseEvents(
                        [
                            """{
                            "type": "init",
                            "result": 0,
                            "app_version": "*",
                            "prod": false
                        }"""
                        ]
                    ),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList([action.Sleep(10)]),
        },
    ),
    Scenario(
        "scenario34",
        "Read/Write to database multiple times to exercise the concurrency",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN"),
                    action.Repeated(
                        [
                            action.Parallel(
                                [
                                    action.NewTransfer(
                                        "DROP_PEER_STIMPY", ["/tmp/testfile-small"]
                                    ),
                                    action.PurgeTransfersUntil(0xFFFFFFFF),
                                ]
                            ),
                        ],
                        1000,
                    ),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    # keep peer alive so DNS resolved could resolve
                    action.Sleep(15),
                ]
            ),
        },
    ),
    Scenario(
        "scenario35-1",
        "Try to connect to rapidly disconnecting peer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.AssertNoEventOfType(
                        [
                            event.FinishFailedTransfer,
                        ],
                        duration=4,
                    ),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Repeated(
                        [
                            action.Start(
                                "DROP_PEER_STIMPY", dbpath="/tmp/db/35-stimpy.sqlite"
                            ),
                            action.SleepMs(30),
                            action.Stop(),
                        ],
                        100,
                    ),
                ]
            ),
        },
        tags=["racy", "offline"],
    ),
    Scenario(
        "scenario35-2",
        "Initiate a transfer to a rapidly disconnecting peer but eventually the file should be transfered succesfully ",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(latency="10ms"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File(
                                        FILES["testfile-small"].id,
                                        "testfile-small",
                                        1048576,
                                    ),
                                },
                            ),
                        ]
                    ),
                    action.Repeated(
                        [
                            action.Sleep(10),
                            action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                        ],
                        6,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-small"].id,
                            ),
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/35-2-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Repeated(
                        [
                            action.Stop(),
                            action.Start(
                                "DROP_PEER_STIMPY", dbpath="/tmp/db/35-2-stimpy.sqlite"
                            ),
                            action.Sleep(2),
                        ],
                        10,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-small"].id,
                                "/tmp/received/testfile-small",
                            )
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario35-3",
        "Report peer status rapidly multiple times, expect success",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(latency="100ms"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File(
                                        FILES["testfile-small"].id,
                                        "testfile-small",
                                        1048576,
                                    ),
                                },
                            ),
                        ]
                    ),
                    action.Repeated(
                        [
                            action.Sleep(0.01),
                            action.SetPeerState("DROP_PEER_STIMPY", PeerState.Offline),
                            action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                        ],
                        500,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-small"].id,
                            ),
                        ]
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-small"].id,
                                "/tmp/received/testfile-small",
                            )
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Sleep(8),  # give time for the signal to be sent
                    action.Stop(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario36",
        "Call stop() right after the download() call. Expect no errors",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Sleep(1),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.ConfigureNetwork(),
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/36",
                    ),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario37-2",
        "Expect no activity when there's a failure in starting libdrop",
        {
            "DROP_PEER_REN": ActionList(
                [
                    # create a transfer so there would be transfer to be re-sent
                    action.Start("DROP_PEER_REN", "/tmp/data.base"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Stop(),
                    # block the port on the interface libdrop's working on
                    action.ListenOnPort("DROP_PEER_REN"),
                    # try again and expect no events and no activity
                    action.ExpectError(
                        action.Start("DROP_PEER_REN", "/tmp/data.base"),
                        ReturnCodes.ADDR_IN_USE,
                    ),
                    action.NoEvent(),
                    action.ExpectError(action.Stop(), ReturnCodes.NOT_STARTED),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(4),
                    action.Start("DROP_PEER_STIMPY"),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
        tags=["basic"],
    ),
    Scenario(
        "scenario37-3",
        "Succeed the transfer then try to restart the receiver, expect no events",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", "/tmp/data.base"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-big"].id,
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    # Peer is online for a few seconds and then starts libdrop instance, expect nothing
                    action.Start("DROP_PEER_STIMPY", dbpath="/tmp/37-3-stimpy.db"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/testfile-big",
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                    action.NoEvent(),
                    action.Start("DROP_PEER_STIMPY", dbpath="/tmp/37-3-stimpy.db"),
                    action.Stop(),
                    action.NoEvent(),
                ]
            ),
        },
        tags=["offline"],
    ),
    Scenario(
        "scenario38",
        "Cancel the transfers from the receiver while the sender is offline and file were in flight, but the receiver did not catched the disconnection yet. Expect clean cancelation",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(latency="1000ms"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", "/tmp/db/38.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Stop(),
                    action.Wait(event.Paused(0, FILES["testfile-big"].id)),
                    action.SleepMs(400),
                    action.Start("DROP_PEER_REN", "/tmp/db/38.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Wait(event.FinishTransferCanceled(0, True)),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id, "testfile-big", 10485760
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received/38",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.SleepMs(400),
                    action.CancelTransferRequest([0]),
                    action.Wait(event.FinishTransferCanceled(0, False)),
                    action.NoEvent(duration=10),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario39-1",
        "Produce a situation in which the receiver has stalled transfer, but sender does not have it anymore. Expect receiver to check the transfer state eventually",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.SleepMs(200),
                    action.Stop(),
                    action.Start(
                        "DROP_PEER_REN"
                    ),  # starting with in memory database, effectively loosing all the data
                    action.Sleep(61),  # 60s is a check request interval
                    action.AssertTransfers([]),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.ExpectCancel([0], True),
                    # TODO: would be nice if peer ID could be matched dynamically with peer_resolver
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "peer_id": "*",
                        "created_at": "*",
                        "states": [
                            {
                                "created_at": "*",
                                "state": "cancel",
                                "by_peer": true
                            }
                        ],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "bytes_received": 0,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                ]
            ),
        },
    ),
    Scenario(
        "scenario39-2",
        "Wait for the receiver -> sender GET /check request in case of alive transfer. Expect the check to not cancel the transfer",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.NoEvent(
                        duration=61
                    ),  # the check interval is 60s so wait a bit longer
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "peer_id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "outgoing",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "base_path": "/tmp",
                                "bytes": 1048576,
                                "bytes_sent": 0,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.NoEvent(
                        duration=61
                    ),  # the check interval is 60s so wait a bit longer
                    action.AssertTransfers(
                        [
                            """{
                        "id": "*",
                        "peer_id": "*",
                        "created_at": "*",
                        "states": [],
                        "type": "incoming",
                        "paths": [
                            {
                                "relative_path": "testfile-small",
                                "bytes": 1048576,
                                "bytes_received": 0,
                                "states": []
                            }
                        ]
                    }"""
                        ]
                    ),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario40-1",
        "Check if the newly created database has 0600 permissions",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/40-1-ren.sqlite"),
                    action.Stop(),
                    action.CheckFilePermissions("/tmp/db/40-1-ren.sqlite", 0o600),
                ]
            ),
        },
    ),
    Scenario(
        "scenario40-2",
        "Check if the existing database permissions are changed to 0600",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/40-2-ren.sqlite"),
                    action.Stop(),
                    action.SetFilePermissions("/tmp/db/40-2-ren.sqlite", 0o644),
                    action.CheckFilePermissions("/tmp/db/40-2-ren.sqlite", 0o644),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/40-2-ren.sqlite"),
                    action.Stop(),
                    action.CheckFilePermissions("/tmp/db/40-2-ren.sqlite", 0o600),
                ]
            ),
        },
    ),
    Scenario(
        "scenario41",
        "Send transfer request with known same UUID as the previously cancelled transfer. Expect the receiver canceling the new one right away",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/41-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    # restart so database writes would be flushed before copying
                    action.Stop(),
                    # Make a copy of database
                    action.CopyFile(
                        "/tmp/db/41-ren.sqlite", "/tmp/db/41-ren-copy.sqlite"
                    ),
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/41-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Stop(),
                    # Start again but this time with a copy of the database. The transfer should be again retried but the other peer should respond with already cancelled
                    action.Start("DROP_PEER_REN", dbpath="/tmp/db/41-ren-copy.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY", dbpath="/tmp/db/41-ren.sqlite"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.Sleep(
                        10
                    ),  # Give enough time for communication and to silently respond to the sender back that transfer was canceled
                    action.Stop(),
                ]
            ),
        },
        tags=["cancel"],
    ),
    Scenario(
        "scenario42-1",
        "Cancel the transfer while on the receiver while the sender is offline",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN", "/tmp/db/42-1-ren.sqlite"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.SleepMs(500),
                    action.Stop(),
                    action.Sleep(1),
                    action.Start("DROP_PEER_REN", "/tmp/db/42-1-ren.sqlite"),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Sleep(1),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Sleep(2),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "cancel"],
    ),
    Scenario(
        "scenario42-2",
        "Cancel the transfer while on the sender while the receiver is offline",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.SleepMs(500),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Sleep(2),
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.Sleep(2),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/42-2-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Stop(),
                    action.Sleep(1),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/42-2-stimpy.sqlite"
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "cancel"],
    ),
    Scenario(
        "scenario43",
        "Sucesfully transfer the file. Then restart the receiver. Expect no events on receiver reconnection",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.NoEvent(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY", dbpath="/tmp/db/43-stimpy.sqlite"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.SleepMs(500),
                    action.Stop(),
                    action.Start("DROP_PEER_STIMPY", dbpath="/tmp/db/43-stimpy.sqlite"),
                    action.NoEvent(4),
                    action.Stop(),
                ]
            ),
        },
        tags=["ofline"],
    ),
    Scenario(
        "scenario44-1",
        "Initiate transfer to multiple peers, all parties are online",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(latency="10ms"),
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.WaitForAnotherPeer("DROP_PEER_GEORGE"),
                    action.NewTransfer("DROP_PEER_GEORGE", ["/tmp/testfile-big"]),
                    action.WaitRacy(
                        [
                            event.Queued(
                                0,
                                {
                                    event.File(
                                        FILES["testfile-small"].id,
                                        "testfile-small",
                                        1048576,
                                    ),
                                },
                            ),
                            event.Queued(
                                1,
                                {
                                    event.File(
                                        FILES["testfile-big"].id,
                                        "testfile-big",
                                        10485760,
                                    ),
                                },
                            ),
                            event.Start(0, FILES["testfile-small"].id),
                            event.Start(1, FILES["testfile-big"].id),
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["testfile-big"].id,
                            ),
                        ],
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Sleep(2),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_GEORGE": ActionList(
                [
                    action.Start("DROP_PEER_GEORGE"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/testfile-big",
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
        },
        tags=["basic", "multiple-peers"],
    ),
    Scenario(
        "scenario44-2",
        "Initiate transfer to multiple offline peers, then both of them come online and all goes well till finish",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(latency="1ms"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        ),
                    ),
                    action.NewTransfer("DROP_PEER_GEORGE", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        ),
                    ),
                    action.NoEvent(),
                    action.Sleep(5),  # synchronize
                    # TODO: make it enum'y and type safe
                    action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                    action.SetPeerState("DROP_PEER_GEORGE", PeerState.Online),
                    action.WaitRacy(
                        [
                            event.Start(0, FILES["testfile-small"].id),
                            event.Start(1, FILES["testfile-big"].id),
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["testfile-big"].id,
                            ),
                        ],
                    ),
                    # TODO if ExpectCancel is taking an array, it makes sense for CancelTransferRequest to take in an array as well
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(5),
                    action.Start("DROP_PEER_STIMPY"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_GEORGE": ActionList(
                [
                    action.Sleep(5),
                    action.Start("DROP_PEER_GEORGE"),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-big"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-big"].id,
                            "/tmp/received/testfile-big",
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "multiple-peers"],
    ),
    Scenario(
        "scenario44-3",
        "Initiate transfer to multiple peers and they will come and go online/offline multiple times",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(latency="1ms"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        ),
                    ),
                    action.NewTransfer("DROP_PEER_GEORGE", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        ),
                    ),
                    action.Repeated(
                        [
                            action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                            action.SetPeerState("DROP_PEER_GEORGE", PeerState.Online),
                            action.Sleep(1),
                        ],
                        20,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["testfile-big"].id,
                            ),
                        ]
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(4),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/44-3-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Sleep(5),  # synchronize
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Repeated(
                        [
                            action.Stop(),
                            action.Start(
                                "DROP_PEER_STIMPY", dbpath="/tmp/db/44-3-stimpy.sqlite"
                            ),
                            action.Sleep(2),
                        ],
                        5,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-small"].id,
                                "/tmp/received/testfile-small",
                            )
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_GEORGE": ActionList(
                [
                    action.Sleep(4),
                    action.Start(
                        "DROP_PEER_GEORGE", dbpath="/tmp/db/44-3-george.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Sleep(5),  # synchronize
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Repeated(
                        [
                            action.Stop(),
                            action.Start(
                                "DROP_PEER_GEORGE", dbpath="/tmp/db/44-3-george.sqlite"
                            ),
                            action.Sleep(2),
                        ],
                        5,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-big"].id,
                                "/tmp/received/testfile-big",
                            )
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "multiple-peers"],
    ),
    Scenario(
        "scenario44-4",
        "Initiate transfer to multiple peers that are online and they will come and go online/offline multiple times",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.ConfigureNetwork(latency="1ms"),
                    action.Start("DROP_PEER_REN"),
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        ),
                    ),
                    # TODO: for such cases it would make sense to introduce "signals" for all of the peers to synchronize on
                    action.WaitForAnotherPeer("DROP_PEER_GEORGE"),
                    action.NewTransfer("DROP_PEER_GEORGE", ["/tmp/testfile-big"]),
                    action.Wait(
                        event.Queued(
                            1,
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        ),
                    ),
                    action.Repeated(
                        [
                            action.SetPeerState("DROP_PEER_STIMPY", PeerState.Online),
                            action.SetPeerState("DROP_PEER_GEORGE", PeerState.Online),
                            action.Sleep(1),
                        ],
                        20,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileUploaded(
                                0,
                                FILES["testfile-small"].id,
                            ),
                            event.FinishFileUploaded(
                                1,
                                FILES["testfile-big"].id,
                            ),
                        ]
                    ),
                    action.CancelTransferRequest([0, 1]),
                    action.ExpectCancel([0, 1], False),
                    action.NoEvent(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Sleep(4),
                    action.Start(
                        "DROP_PEER_STIMPY", dbpath="/tmp/db/44-3-stimpy.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Sleep(5),  # synchronize
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Repeated(
                        [
                            action.Stop(),
                            action.Start(
                                "DROP_PEER_STIMPY", dbpath="/tmp/db/44-3-stimpy.sqlite"
                            ),
                            action.Sleep(2),
                        ],
                        5,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-small"].id,
                                "/tmp/received/testfile-small",
                            )
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_GEORGE": ActionList(
                [
                    action.Sleep(4),
                    action.Start(
                        "DROP_PEER_GEORGE", dbpath="/tmp/db/44-3-george.sqlite"
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-big"].id,
                                    "testfile-big",
                                    10485760,
                                ),
                            },
                        )
                    ),
                    action.Sleep(5),  # synchronize
                    action.Download(
                        0,
                        FILES["testfile-big"].id,
                        "/tmp/received",
                    ),
                    action.Repeated(
                        [
                            action.Stop(),
                            action.Start(
                                "DROP_PEER_GEORGE", dbpath="/tmp/db/44-3-george.sqlite"
                            ),
                            action.Sleep(2),
                        ],
                        5,
                    ),
                    action.WaitAndIgnoreExcept(
                        [
                            event.FinishFileDownloaded(
                                0,
                                FILES["testfile-big"].id,
                                "/tmp/received/testfile-big",
                            )
                        ]
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-big", 10485760),
                        ],
                    ),
                    action.ExpectCancel([0], True),
                    action.Stop(),
                ]
            ),
        },
        tags=["offline", "multiple-peers"],
    ),
    Scenario(
        "scenario44",
        "Check if the transfer request and cancelation are suppressed within a huge latency network",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.ConfigureNetwork(latency="1000ms"),
                    action.Start("DROP_PEER_REN"),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.Sleep(6),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.NoEvent(duration=8),
                    action.Stop(),
                ]
            ),
        },
    ),
    Scenario(
        "scenario45",
        "Start instace twice on sender and receiver. Then expect transfer to work properly",
        {
            "DROP_PEER_REN": ActionList(
                [
                    action.WaitForAnotherPeer("DROP_PEER_STIMPY"),
                    action.Start("DROP_PEER_REN"),
                    action.ExpectError(
                        action.Start("DROP_PEER_REN"), ReturnCodes.INSTANCE_START
                    ),
                    action.NewTransfer("DROP_PEER_STIMPY", ["/tmp/testfile-small"]),
                    action.Wait(
                        event.Queued(
                            0,
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileUploaded(
                            0,
                            FILES["testfile-small"].id,
                        )
                    ),
                    action.ExpectCancel([0], True),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
            "DROP_PEER_STIMPY": ActionList(
                [
                    action.Start("DROP_PEER_STIMPY"),
                    action.ExpectError(
                        action.Start("DROP_PEER_STIMPY"), ReturnCodes.INSTANCE_START
                    ),
                    action.Wait(
                        event.Receive(
                            0,
                            "DROP_PEER_REN",
                            {
                                event.File(
                                    FILES["testfile-small"].id,
                                    "testfile-small",
                                    1048576,
                                ),
                            },
                        )
                    ),
                    action.Download(
                        0,
                        FILES["testfile-small"].id,
                        "/tmp/received",
                    ),
                    action.Wait(event.Start(0, FILES["testfile-small"].id)),
                    action.Wait(
                        event.FinishFileDownloaded(
                            0,
                            FILES["testfile-small"].id,
                            "/tmp/received/testfile-small",
                        )
                    ),
                    action.CheckDownloadedFiles(
                        [
                            action.File("/tmp/received/testfile-small", 1048576),
                        ],
                    ),
                    action.CancelTransferRequest([0]),
                    action.ExpectCancel([0], False),
                    action.NoEvent(),
                    action.Stop(),
                ]
            ),
        },
    ),
]
