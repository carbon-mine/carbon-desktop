// SPDX-License-Identifier: MPL-2.0
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io::{self, Error},
    mem::size_of,
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
    ptr::{null, null_mut},
    slice,
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        AddAce,
        Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SetSecurityInfo,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        FreeSid, GetAce, GetLengthSid, GetSecurityDescriptorControl, InitializeAcl,
        Isolation::DeriveAppContainerSidFromAppContainerName,
        ACL, CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, INHERITED_ACE, INHERIT_ONLY_ACE,
        LABEL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE, SECURITY_ATTRIBUTES, SE_DACL_PROTECTED,
    },
    Storage::FileSystem::{
        CreateDirectoryW, FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
        GetVolumeInformationByHandleW, BY_HANDLE_FILE_INFORMATION, DELETE,
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_ID_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_TRAVERSE,
    },
    System::SystemServices::{
        ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE, MAXIMUM_ALLOWED,
        SYSTEM_MANDATORY_LABEL_ACE_TYPE,
    },
};

const DIR_ACCESS: u32 = FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_TRAVERSE | FILE_DELETE_CHILD;
const FILE_ACCESS: u32 = FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE;
const LOW_SID: [u8; 12] = [1, 1, 0, 0, 0, 0, 0, 16, 0, 16, 0, 0];
const NO_WRITE_UP: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    volume: u64,
    id: [u8; 16],
}

pub(super) struct Object {
    pub file: File,
    pub identity: Identity,
    pub directory: bool,
}

pub(super) fn open(path: &Path, write: bool) -> io::Result<Object> {
    let sharing = FILE_SHARE_READ | FILE_SHARE_WRITE | if write { 0 } else { FILE_SHARE_DELETE };
    open_scoped(path, write, sharing)
}

pub(super) fn pin(path: &Path) -> io::Result<Object> {
    open_scoped(path, false, FILE_SHARE_READ | FILE_SHARE_WRITE)
}

fn open_scoped(path: &Path, write: bool, sharing: u32) -> io::Result<Object> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(sharing)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
    if write {
        // MAXIMUM_ALLOWED suppresses SetSecurityInfo's recursive inheritance walk.
        options.access_mode(MAXIMUM_ALLOWED);
    }
    let file = options.open(path)?;
    object(file)
}

pub(super) fn object(file: File) -> io::Result<Object> {
    let handle = file.as_raw_handle();
    let mut attrs = BY_HANDLE_FILE_INFORMATION::default();
    // The borrowed handle and output buffer remain live through both queries.
    checked(unsafe { GetFileInformationByHandle(handle, &mut attrs) })?;
    let directory = attrs.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    if attrs.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || (!directory && attrs.nNumberOfLinks != 1)
    {
        return Err(Error::other(
            "The protected state contains a reparse point or hard link.",
        ));
    }
    let mut id = FILE_ID_INFO::default();
    checked(unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    })?;
    Ok(Object {
        file,
        directory,
        identity: Identity {
            volume: id.VolumeSerialNumber,
            id: id.FileId.Identifier,
        },
    })
}

pub(super) fn require_ntfs(file: &File) -> io::Result<()> {
    let mut fs = [0u16; 16];
    // Only the filesystem name output is requested.
    checked(unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    })?;
    if fs[..5] != [78, 84, 70, 83, 0] {
        return Err(Error::other(
            "Protected Windows state requires a local NTFS directory.",
        ));
    }
    Ok(())
}

pub(super) fn create_low_directory(path: &Path) -> io::Result<()> {
    let descriptor = "S:(ML;OICI;NW;;;LW)"
        .encode_utf16()
        .chain([0])
        .collect::<Vec<_>>();
    let mut pointer = null_mut();
    // This descriptor adds only an inheritable low mandatory label. The new
    // directory's normal user/group DACL is inherited from its app-owned parent.
    checked(unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            descriptor.as_ptr(),
            SDDL_REVISION_1,
            &mut pointer,
            null_mut(),
        )
    })?;
    let block = LocalBlock(pointer);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: block.0,
        bInheritHandle: 0,
    };
    let path: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    checked(unsafe { CreateDirectoryW(path.as_ptr(), &attributes) })
}

pub(super) fn run_sid(label: &str) -> io::Result<Vec<u8>> {
    let name: Vec<_> = label.encode_utf16().chain([0]).collect();
    let mut sid = null_mut();
    // The documented derivation neither creates a profile nor changes access.
    let hr = unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) };
    if hr < 0 || sid.is_null() {
        return Err(Error::from_raw_os_error(hr));
    }
    // The API returned a valid SID. Copy its bounded native representation.
    let length = unsafe { GetLengthSid(sid) } as usize;
    let result = if (12..=68).contains(&length) {
        Ok(unsafe { slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec())
    } else {
        Err(Error::other("Unexpected AppContainer SID."))
    };
    unsafe { FreeSid(sid) };
    result
}

struct LocalBlock(*mut c_void);

impl Drop for LocalBlock {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

#[derive(Clone)]
pub(super) struct Acl {
    revision: u8,
    protected: bool,
    entries: Vec<Vec<u8>>,
}

impl Acl {
    pub fn read(file: &File) -> io::Result<Self> {
        let (block, acl) = read_security(file, false)?;
        let mut control = 0;
        let mut sd_revision = 0;
        checked(unsafe { GetSecurityDescriptorControl(block.0, &mut control, &mut sd_revision) })?;
        let (revision, entries) = entries(acl)?;
        if entries.iter().any(|ace| {
            !matches!(
                u32::from(ace[0]),
                ACCESS_ALLOWED_ACE_TYPE | ACCESS_DENIED_ACE_TYPE
            )
        }) {
            return Err(Error::other(
                "The protected state has an unsupported access control entry.",
            ));
        }
        Ok(Self {
            revision,
            protected: control & SE_DACL_PROTECTED != 0,
            entries,
        })
    }

    pub fn digest(&self) -> [u8; 32] {
        let mut sha = Sha256::new();
        sha.update([self.revision, self.protected.into()]);
        for ace in &self.entries {
            sha.update(ace);
        }
        sha.finalize().into()
    }

    pub fn granted(&self, sid: &[u8], directory: bool) -> io::Result<Self> {
        if self.entries.iter().any(|ace| &ace[8..] == sid) {
            return Err(Error::other(
                "This run SID already has an access control entry.",
            ));
        }
        let mut next = self.clone();
        let index = next
            .entries
            .iter()
            .position(|ace| u32::from(ace[1]) & INHERITED_ACE != 0)
            .unwrap_or(next.entries.len());
        let owned = if directory {
            vec![
                allowed(sid, DIR_ACCESS, CONTAINER_INHERIT_ACE),
                allowed(sid, FILE_ACCESS, OBJECT_INHERIT_ACE | INHERIT_ONLY_ACE),
            ]
        } else {
            vec![allowed(sid, FILE_ACCESS, 0)]
        };
        next.entries.splice(index..index, owned);
        Ok(next)
    }

    pub fn require_private_package_acl(&self) -> io::Result<()> {
        // S-1-15 is the application package authority. Preserve its existing
        // entries by refusing this scope if an allow ACE already grants one.
        if self.entries.iter().any(|ace| {
            ace[0] == ACCESS_ALLOWED_ACE_TYPE as u8 && ace.get(10..16) == Some(&[0, 0, 0, 0, 0, 15])
        }) {
            return Err(Error::other(
                "Protected state already grants access to an app package or capability.",
            ));
        }
        Ok(())
    }

    pub fn stripped(&self, sid: &[u8], directory: bool, inherited_only: bool) -> io::Result<Self> {
        let mut next = self.clone();
        for ace in self.entries.iter().filter(|ace| &ace[8..] == sid) {
            let mask = u32::from_le_bytes(ace[4..8].try_into().unwrap());
            let flags = u32::from(ace[1]);
            let inherited = flags & INHERITED_ACE != 0;
            let shape = flags & !INHERITED_ACE;
            let acceptable = if directory {
                (mask == DIR_ACCESS && shape == CONTAINER_INHERIT_ACE)
                    || (mask == FILE_ACCESS && shape == (OBJECT_INHERIT_ACE | INHERIT_ONLY_ACE))
            } else {
                mask == FILE_ACCESS && shape == 0
            };
            if !acceptable
                || ace[0] != ACCESS_ALLOWED_ACE_TYPE as u8
                || (inherited_only && !inherited)
            {
                return Err(Error::other(
                    "The protected state has unexpected rights for the run SID.",
                ));
            }
        }
        next.entries.retain(|ace| &ace[8..] != sid);
        Ok(next)
    }

    pub fn write(&self, file: &File) -> io::Result<()> {
        let bytes = 8 + self.entries.iter().map(Vec::len).sum::<usize>();
        if bytes > u16::MAX as usize {
            return Err(Error::other(
                "The protected state access list exceeds its bound.",
            ));
        }
        let mut buffer = vec![0u32; bytes.div_ceil(4)];
        let acl = buffer.as_mut_ptr().cast::<ACL>();
        // Win32 ACLs are DWORD-aligned. All ACE buffers were structurally checked.
        checked(unsafe { InitializeAcl(acl, bytes as u32, u32::from(self.revision)) })?;
        for ace in &self.entries {
            checked(unsafe {
                AddAce(
                    acl,
                    u32::from(self.revision),
                    u32::MAX,
                    ace.as_ptr().cast(),
                    ace.len() as u32,
                )
            })?;
        }
        // These handles have MAXIMUM_ALLOWED access, which suppresses propagation.
        // Only DACL data is written. Owner, group, auditing and integrity are untouched.
        win32(unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null(),
            )
        })
    }
}

pub(super) fn require_low(file: &File) -> io::Result<()> {
    let (_block, acl) = read_security(file, true)?;
    let (_, list) = entries(acl)?;
    let valid = list
        .iter()
        .filter(|ace| u32::from(ace[0]) == SYSTEM_MANDATORY_LABEL_ACE_TYPE)
        .all(|ace| {
            ace[8..] == LOW_SID && u32::from_le_bytes(ace[4..8].try_into().unwrap()) == NO_WRITE_UP
        })
        && list
            .iter()
            .any(|ace| u32::from(ace[0]) == SYSTEM_MANDATORY_LABEL_ACE_TYPE);
    if !valid {
        return Err(Error::other(
            "The protected state object does not have the expected low integrity policy.",
        ));
    }
    Ok(())
}

fn read_security(file: &File, label: bool) -> io::Result<(LocalBlock, *mut ACL)> {
    let mut pointer = null_mut();
    let mut acl: *mut ACL = null_mut();
    let (info, dacl, sacl) = if label {
        (
            LABEL_SECURITY_INFORMATION,
            null_mut(),
            &mut acl as *mut *mut ACL,
        )
    } else {
        (
            DACL_SECURITY_INFORMATION,
            &mut acl as *mut *mut ACL,
            null_mut(),
        )
    };
    // The returned ACL pointer borrows Win32's returned descriptor allocation.
    win32(unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            info,
            null_mut(),
            null_mut(),
            dacl,
            sacl,
            &mut pointer,
        )
    })?;
    let block = LocalBlock(pointer);
    if acl.is_null() {
        return Err(Error::other(
            "The protected state lacks an explicit security list.",
        ));
    }
    Ok((block, acl))
}

fn entries(acl: *const ACL) -> io::Result<(u8, Vec<Vec<u8>>)> {
    // This pointer came from GetSecurityInfo and is backed by a live descriptor.
    let header = unsafe { &*acl };
    if header.AclSize < 8 || header.AceCount > 256 {
        return Err(Error::other(
            "The protected state security list exceeds its bound.",
        ));
    }
    let base = acl as usize;
    let end = base + usize::from(header.AclSize);
    let mut list = Vec::with_capacity(usize::from(header.AceCount));
    for index in 0..u32::from(header.AceCount) {
        let mut pointer = null_mut();
        checked(unsafe { GetAce(acl, index, &mut pointer) })?;
        let address = pointer as usize;
        if address < base + 8 || address + 8 > end {
            return Err(Error::other(
                "The protected state security list is malformed.",
            ));
        }
        let prefix = unsafe { slice::from_raw_parts(pointer.cast::<u8>(), 8) };
        let length = usize::from(u16::from_le_bytes([prefix[2], prefix[3]]));
        if length < 8 || address + length > end || length % 4 != 0 {
            return Err(Error::other(
                "The protected state security entry is malformed.",
            ));
        }
        list.push(unsafe { slice::from_raw_parts(pointer.cast::<u8>(), length) }.to_vec());
    }
    Ok((header.AclRevision, list))
}

fn allowed(sid: &[u8], mask: u32, flags: u32) -> Vec<u8> {
    let mut bytes = vec![ACCESS_ALLOWED_ACE_TYPE as u8, flags as u8];
    bytes.extend_from_slice(&((8 + sid.len()) as u16).to_le_bytes());
    bytes.extend_from_slice(&mask.to_le_bytes());
    bytes.extend_from_slice(sid);
    bytes
}

fn checked(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(Error::last_os_error())
    } else {
        Ok(())
    }
}

fn win32(result: u32) -> io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(Error::from_raw_os_error(result as i32))
    }
}
