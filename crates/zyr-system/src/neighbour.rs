//! The card a computer on this network answers with.
//!
//! What waking a computer by the network needs: the address of its card,
//! which it does not announce, and which this computer learnt anyway the
//! moment the two spoke. Windows keeps it in its table of neighbours.

use std::net::Ipv4Addr;

use windows_sys::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIpNetTable2, MIB_IPNET_ROW2, MIB_IPNET_TABLE2,
};
use windows_sys::Win32::Networking::WinSock::AF_INET;

/// The hardware address of the card answering on that address of this
/// network, as Windows last heard it.
pub fn card_of(address: Ipv4Addr) -> Option<[u8; 6]> {
    let mut table: *mut MIB_IPNET_TABLE2 = std::ptr::null_mut();
    // SAFETY: a slot of ours for the table the system makes, given back
    // below.
    if unsafe { GetIpNetTable2(AF_INET, &mut table) } != 0 || table.is_null() {
        return None;
    }
    // SAFETY: the table the call above made, read for as many rows as it
    // says it holds, before it is given back.
    let found = unsafe {
        let rows = std::slice::from_raw_parts(
            (&raw const (*table).Table).cast::<MIB_IPNET_ROW2>(),
            (*table).NumEntries as usize,
        );
        rows.iter()
            .find(|row| {
                row.Address.si_family == AF_INET
                    && row.Address.Ipv4.sin_addr.S_un.S_addr == u32::from_ne_bytes(address.octets())
                    && row.PhysicalAddressLength == 6
            })
            .map(|row| {
                let mut card = [0u8; 6];
                card.copy_from_slice(&row.PhysicalAddress[..6]);
                card
            })
    };
    // SAFETY: the table made above, given back once.
    unsafe { FreeMibTable(table.cast()) };
    found.filter(|card| *card != [0; 6])
}
