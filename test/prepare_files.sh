#!/bin/bash

# Create directories
mkdir -p /tmp/received/symtest-files
mkdir -p /tmp/db
mkdir -p /tmp/deep/path
mkdir -p /tmp/deep/another-path
mkdir -p /tmp/nested/big
mkdir -p /tmp/duplicate
mkdir -p /tmp/name
mkdir -p /tmp/different/name
mkdir -p /tmp/empty-dir
mkdir -p /tmp/empty-dir/one
mkdir -p /tmp/empty-dir/one/two
mkdir -p /tmp/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
mkdir -p /tmp/dir-with-invalid_char-\<
mkdir -p /tmp/dir-with-invalid_char-\>

# FILES dictionary
dd bs=1024K count=1 if=/dev/urandom of="/tmp/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.txt"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/testfile.txt"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/testfile-small"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-big"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/deep/path/file1.ext1"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/deep/path/file2.ext2"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/deep/another-path/file3.ext3"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/deep/another-path/file4.ext4"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-01"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-02"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-03"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-04"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-05"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-06"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-07"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-08"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-09"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/testfile-bulk-10"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/nested/big/testfile-01"
dd bs=10240K count=1 if=/dev/urandom of="/tmp/nested/big/testfile-02"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/testfile.small.with.complicated.extension"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/with-illegal-char-
-"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/duplicate/testfile-small"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/duplicate/testfile.small.with.complicated.extension"
dd bs=10240K count=2 if=/dev/urandom of="/tmp/duplicate/testfile-big"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/name/file-01"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/different/name/file-02"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/utf8-testfile-宁察"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/dir-with-invalid_char-</file-01"
dd bs=1024K count=1 if=/dev/urandom of="/tmp/dir-with-invalid_char->/file-01"

# Create tiny jpeg and gif files to test analytics
# "/tmp/tiny-jpeg.jpg"
echo -ne "\xFF\xD8\xFF\xE0\x00\x10\x4A\x46\x49\x46\x00\x01\x01\x01\x00\x48\x00\x48\x00\x00\xFF\xDB\x00\x43\x00\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xC2\x00\x0B\x08\x00\x01\x00\x01\x01\x01\x11\x00\xFF\xC4\x00\x14\x10\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xFF\xDA\x00\x08\x01\x01\x00\x01\x3F\x10" > "/tmp/tiny-jpeg.jpg"
# "/tmp/tiny-gif.gif"
echo -ne "\x47\x49\x46\x38\x39\x61\x01\x00\x01\x00\x00\xff\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x00\x3b" > "/tmp/tiny-gif.gif"

touch "/tmp/zero-sized-file"

# Symlinks
# Create symlinks from the symlinks dictionary
ln -s "/tmp/this-file-does-not-exists.ext" "/tmp/received/symtest-files/testfile-small"
ln -s "/tmp/this-dir-does-not-exists" "/tmp/received/symtest-dir"

# DBFILES
# Create a corrupted SQLite file (as per DBFILES dictionary)
echo "this is a corrupted sqlite file" > "/tmp/db/26-1-corrupted.sqlite"


# Done
echo "Files, symlinks, and SQLite files created."
