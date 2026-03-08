#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use windows::Win32::Foundation::CloseHandle;
#[cfg(windows)]
use windows::Win32::System::ProcessStatus::GetModuleFileNameExW;
#[cfg(windows)]
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_VM_READ,
};

#[cfg(windows)]
fn process_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

#[cfg(windows)]
fn query_process_path_with_access(
    process_id: u32,
    desired_access: PROCESS_ACCESS_RIGHTS,
) -> Option<String> {
    unsafe {
        let handle = OpenProcess(desired_access, false, process_id).ok()?;
        let mut buffer = [0u16; 260];
        let len = GetModuleFileNameExW(Some(handle), None, &mut buffer);
        let _ = CloseHandle(handle);

        if len > 0 {
            Some(String::from_utf16_lossy(&buffer[..len as usize]))
        } else {
            None
        }
    }
}

#[cfg(windows)]
pub fn query_process_path(process_id: u32) -> Option<String> {
    query_process_path_with_access(process_id, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ)
        .or_else(|| query_process_path_with_access(process_id, PROCESS_QUERY_LIMITED_INFORMATION))
}

#[cfg(windows)]
pub fn query_process_path_and_name(process_id: u32) -> Option<(String, String)> {
    let path = query_process_path(process_id)?;
    let name = process_name_from_path(&path);
    Some((path, name))
}
