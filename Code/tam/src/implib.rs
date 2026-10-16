//! DLL 임포트 라이브러리(.lib) 생성기.
//!
//! Windows SDK 없이 `kernel32.dll` 같은 DLL의 함수를 링크하려고, 이름 목록만으로 COFF 아카이브를
//! 직접 만든다. 멤버는 "short import object"(함수당 하나)이고, 링커가 `__imp_이름`(IAT 슬롯)과
//! `이름`(점프 thunk) 두 심볼을 찾을 수 있게 아카이브 심볼 테이블(두 개의 linker member)을 쓴다.

/// `dll`에서 `names`를 가져오는 x64 임포트 라이브러리의 바이트열.
pub fn import_lib(dll: &str, names: &[&str]) -> Vec<u8> {
    // 멤버 본문: 20바이트 헤더 + "이름\0" + "dll\0"
    let members: Vec<Vec<u8>> = names
        .iter()
        .map(|n| {
            let size = (n.len() + 1 + dll.len() + 1) as u32;
            let mut m = Vec::new();
            m.extend_from_slice(&0u16.to_le_bytes()); // Sig1
            m.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Sig2
            m.extend_from_slice(&0u16.to_le_bytes()); // Version
            m.extend_from_slice(&0x8664u16.to_le_bytes()); // Machine = AMD64
            m.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
            m.extend_from_slice(&size.to_le_bytes()); // SizeOfData
            m.extend_from_slice(&0u16.to_le_bytes()); // Ordinal/Hint
            m.extend_from_slice(&4u16.to_le_bytes()); // Type: CODE(0) | NAME(1) << 2
            m.extend_from_slice(n.as_bytes());
            m.push(0);
            m.extend_from_slice(dll.as_bytes());
            m.push(0);
            m
        })
        .collect();

    // (심볼 이름, 멤버 번호 0-based). 멤버마다 `__imp_이름`과 `이름`.
    let mut syms: Vec<(String, usize)> = Vec::new();
    for (i, n) in names.iter().enumerate() {
        syms.push((format!("__imp_{n}"), i));
        syms.push((n.to_string(), i));
    }

    let pad = |len: usize| len + (len & 1);
    let member_total = |data_len: usize| 60 + pad(data_len);

    // linker member 두 개의 크기를 먼저 계산해 멤버들의 파일 오프셋을 정한다.
    let strtab: usize = syms.iter().map(|(s, _)| s.len() + 1).sum();
    let first_len = 4 + 4 * syms.len() + strtab;
    let second_len = 4 + 4 * members.len() + 4 + 2 * syms.len() + strtab;
    let mut offset = 8 + member_total(first_len) + member_total(second_len);
    let mut offsets = Vec::new();
    for m in &members {
        offsets.push(offset as u32);
        offset += member_total(m.len());
    }

    let mut out = Vec::new();
    out.extend_from_slice(b"!<arch>\n");

    // 첫 번째 linker member: 빅엔디언, 심볼마다 멤버 오프셋.
    let mut first = Vec::new();
    first.extend_from_slice(&(syms.len() as u32).to_be_bytes());
    for (_, mi) in &syms {
        first.extend_from_slice(&offsets[*mi].to_be_bytes());
    }
    for (s, _) in &syms {
        first.extend_from_slice(s.as_bytes());
        first.push(0);
    }
    write_member(&mut out, "/", &first);

    // 두 번째 linker member: 리틀엔디언, 멤버 오프셋 표 + 이름순 정렬된 심볼의 멤버 번호(1-based).
    let mut sorted = syms.clone();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut second = Vec::new();
    second.extend_from_slice(&(members.len() as u32).to_le_bytes());
    for o in &offsets {
        second.extend_from_slice(&o.to_le_bytes());
    }
    second.extend_from_slice(&(sorted.len() as u32).to_le_bytes());
    for (_, mi) in &sorted {
        second.extend_from_slice(&((*mi + 1) as u16).to_le_bytes());
    }
    for (s, _) in &sorted {
        second.extend_from_slice(s.as_bytes());
        second.push(0);
    }
    write_member(&mut out, "/", &second);

    let member_name = format!("{dll}/");
    for m in &members {
        write_member(&mut out, &member_name, m);
    }
    out
}

fn write_member(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    let header = format!(
        "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
        name,
        0, // 날짜
        0, // uid
        0, // gid
        0, // mode
        data.len()
    );
    debug_assert_eq!(header.len(), 60);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(b'\n');
    }
}

/// 런타임이 쓰는 kernel32 함수들. (M11 OS 계층이 늘어나면 여기에 더한다.)
pub const KERNEL32_RUNTIME: &[&str] = &[
    "ExitProcess",
    "GetProcessHeap",
    "HeapAlloc",
    "HeapFree",
    "HeapReAlloc",
];
