//! lld-link가 만든 PE 파일을 "평평한 메모리 이미지"로 펼치고, 부팅 이미지를 만든다 (M12).
//!
//! BIOS/AArch64 대상은 PE 로더가 없으므로 섹션을 RVA 위치에 그대로 펴서 `ImageBase`에 올려 놓는
//! 방식을 쓴다. (고정 주소로 링크하므로 재배치가 필요 없다.)

/// 펼친 이미지.
pub struct Flat {
    /// RVA 0부터 시작하는 메모리 이미지. 섹션 뒤쪽 BSS는 0으로 채워 포함한다.
    pub bytes: Vec<u8>,
    pub image_base: u64,
    pub entry_rva: u32,
}

fn u16_at(b: &[u8], o: usize) -> Result<u16, String> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or_else(|| "PE file is truncated".to_string())
}

fn u32_at(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "PE file is truncated".to_string())
}

fn u64_at(b: &[u8], o: usize) -> Result<u64, String> {
    b.get(o..o + 8)
        .map(|s| u64::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| "PE file is truncated".to_string())
}

pub fn flatten_pe(pe: &[u8]) -> Result<Flat, String> {
    if pe.get(0..2) != Some(b"MZ") {
        return Err("not a PE file (missing MZ header)".into());
    }
    let nt = u32_at(pe, 0x3C)? as usize;
    if pe.get(nt..nt + 4) != Some(b"PE\0\0") {
        return Err("not a PE file (missing PE signature)".into());
    }
    let nsec = u16_at(pe, nt + 6)? as usize;
    let opt_size = u16_at(pe, nt + 20)? as usize;
    let opt = nt + 24;
    if u16_at(pe, opt)? != 0x20B {
        return Err("expected a PE32+ (64-bit) image".into());
    }
    let entry_rva = u32_at(pe, opt + 16)?;
    let image_base = u64_at(pe, opt + 24)?;
    let mut end = 0usize;
    let mut secs = Vec::new();
    for i in 0..nsec {
        let s = opt + opt_size + i * 40;
        let vsize = u32_at(pe, s + 8)? as usize;
        let va = u32_at(pe, s + 12)? as usize;
        let raw_size = u32_at(pe, s + 16)? as usize;
        let raw_ptr = u32_at(pe, s + 20)? as usize;
        let mem = vsize.max(raw_size);
        end = end.max(va + mem);
        secs.push((va, vsize, raw_ptr, raw_size));
    }
    let mut bytes = vec![0u8; end];
    for (va, vsize, raw_ptr, raw_size) in secs {
        let n = raw_size.min(if vsize == 0 { raw_size } else { vsize });
        if n == 0 {
            continue;
        }
        let src = pe.get(raw_ptr..raw_ptr + n).ok_or("PE section data is truncated")?;
        bytes[va..va + n].copy_from_slice(src);
    }
    Ok(Flat { bytes, image_base, entry_rva })
}

/// BIOS 부트 섹터. `boot/bios.asm`을 nasm으로 조립한 512바이트다.
pub const BIOS_BOOT_SECTOR: &[u8; 512] = include_bytes!("boot_bios.bin");

/// 부트 섹터가 페이로드를 올려 놓는 물리 주소. PE의 ImageBase와 같아야 한다.
pub const BIOS_LOAD_ADDR: u64 = 0x10000;

/// 페이로드가 쓸 수 있는 최대 크기. (뒤쪽에 스택 여유를 남긴다: 0x10000 + 0x60000 = 0x70000)
pub const BIOS_MAX_PAYLOAD: usize = 0x60000;

/// 부팅 가능한 BIOS 디스크 이미지: 부트 섹터 + 섹터 단위로 채운 페이로드.
pub fn bios_disk(flat: &Flat) -> Result<Vec<u8>, String> {
    if flat.image_base != BIOS_LOAD_ADDR {
        return Err(format!("image base is {:#x}, expected {:#x}", flat.image_base, BIOS_LOAD_ADDR));
    }
    if flat.bytes.len() > BIOS_MAX_PAYLOAD {
        return Err(format!(
            "program is too large for the BIOS target: {} bytes (limit {} bytes)",
            flat.bytes.len(),
            BIOS_MAX_PAYLOAD
        ));
    }
    let sectors = flat.bytes.len().div_ceil(512);
    let entry = BIOS_LOAD_ADDR + flat.entry_rva as u64;
    let mut img = BIOS_BOOT_SECTOR.to_vec();
    img[3..5].copy_from_slice(&(sectors as u16).to_le_bytes());
    img[5..9].copy_from_slice(&(entry as u32).to_le_bytes());
    img.extend_from_slice(&flat.bytes);
    img.resize(512 + sectors * 512, 0);
    Ok(img)
}

/// AArch64 Linux `Image` 형식. QEMU `-kernel`이 이 헤더를 알아보고 RAM 시작 + text_offset에 올린다.
pub const AARCH64_IMAGE_BASE: u64 = 0x4008_0000;

pub fn aarch64_image(flat: &Flat) -> Result<Vec<u8>, String> {
    if flat.image_base != AARCH64_IMAGE_BASE {
        return Err(format!("image base is {:#x}, expected {:#x}", flat.image_base, AARCH64_IMAGE_BASE));
    }
    let mut img = flat.bytes.clone();
    if img.len() < 64 {
        img.resize(64, 0);
    }
    // 0x00: `b entry` (엔트리까지의 오프셋을 4로 나눈 26비트 즉시값), 0x04: nop
    if flat.entry_rva % 4 != 0 || flat.entry_rva / 4 >= (1 << 25) {
        return Err("entry point is out of branch range".into());
    }
    let b = 0x1400_0000u32 | (flat.entry_rva / 4);
    img[0..4].copy_from_slice(&b.to_le_bytes());
    img[4..8].copy_from_slice(&0xD503_201Fu32.to_le_bytes());
    img[8..16].copy_from_slice(&0x80000u64.to_le_bytes()); // text_offset
    let size = img.len() as u64;
    img[16..24].copy_from_slice(&size.to_le_bytes()); // image_size
    img[24..32].copy_from_slice(&2u64.to_le_bytes()); // flags: little endian, 4K pages
    img[32..56].fill(0);
    img[56..60].copy_from_slice(b"ARM\x64"); // magic
    img[60..64].fill(0);
    Ok(img)
}
