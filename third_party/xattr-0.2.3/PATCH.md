# xattr 0.2.3 compatibility patch

Source: https://crates.io/crates/xattr/0.2.3
Crate SHA-256: 6d1526bbe5aaeb5eb06885f4d987bcdfa5e23187055de9b83fe00156a821fabc

The pinned GPUI HTTP dependency uses this release through zed-async-tar. On Linux and Android the local patch maps the deprecated ENOATTR compatibility constant to libc::ENODATA. The macOS and BSD mappings retain libc::ENOATTR. See https://github.com/rust-lang/libc/issues/1356 and https://github.com/zed-industries/zed/issues/67211. Retire the patch when the GPUI dependency tree upgrades xattr. Both upstream license notices are included.
