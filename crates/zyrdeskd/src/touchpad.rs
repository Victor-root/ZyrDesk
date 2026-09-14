//! Le pavé tactile, éteint et rallumé pour que Windows relise ses
//! réglages.
//!
//! Ce que Windows fait d'un geste à trois doigts est décidé par trois
//! valeurs de sa page du pavé. ZyrDesk les écrit à zéro le temps d'une
//! session, pour que le geste parte dans la session au lieu d'agir ici,
//! et Windows n'en tient aucun compte : ce qui applique ces gestes a lu
//! ces valeurs une fois et les garde. Une page ouverte à la main les lui
//! fait relire ; rien de ce qu'un programme peut appeler ne le fait.
//!
//! Ce qui le fait relire, c'est le pavé qui s'en va et revient. Un
//! périphérique qui se rattache repart de ses réglages, et ceux-là sont
//! alors les nôtres. C'est brutal, ça coûte une seconde de pavé mort, et
//! c'est la seule chose qui marche sans demander à la personne d'aller
//! régler trois listes déroulantes avant chaque session.
//!
//! Ici et non dans la fenêtre : éteindre un périphérique demande les
//! droits d'un administrateur, que ce service a et que la fenêtre n'a
//! pas. Et rien de ce que la fenêtre dit n'est employé, pas même un nom :
//! ce service cherche lui-même les pavés de précision de la machine et ne
//! touche à rien d'autre. Un nom qui arriverait d'ailleurs serait une
//! façon de faire éteindre à un service ce que son appelant n'a pas le
//! droit d'éteindre.

/// Éteint et rallume les pavés de précision de cette machine.
///
/// Répond ce qu'il faut écrire au journal, ou pourquoi rien n'a pu être
/// fait. Le rallumage est tenté quoi qu'il arrive : un pavé laissé
/// éteint, c'est un portable sans pointeur.
#[cfg(windows)]
pub fn wake_it_again() -> Result<String, String> {
    let pads = the_precision_pads();
    if pads.is_empty() {
        return Err("cet ordinateur n'a pas de pavé tactile de précision.".to_string());
    }
    let mut done = 0usize;
    let mut refused: Vec<String> = Vec::new();
    for pad in &pads {
        match turned_off_and_on(pad) {
            Ok(()) => done += 1,
            Err(why) => refused.push(format!("{pad} : {why}")),
        }
    }
    if done == 0 {
        return Err(format!(
            "aucun pavé n'a pu être redémarré : {}",
            refused.join(" ; ")
        ));
    }
    let mut said = format!("{done} pavé(s) éteint(s) et rallumé(s)");
    if !refused.is_empty() {
        said.push_str(&format!(
            ", et {} refusé(s) : {}",
            refused.len(),
            refused.join(" ; ")
        ));
    }
    Ok(said)
}

#[cfg(not(windows))]
pub fn wake_it_again() -> Result<String, String> {
    Err("les pavés tactiles de précision n'existent que sous Windows".to_string())
}

/// Ce qu'un pavé de précision dit de lui-même dans la table des usages.
#[cfg(windows)]
const A_TOUCHPAD: (u16, u16) = (0x0D, 0x05);

/// Les pavés de précision de cette machine, par l'identifiant sous lequel
/// un installeur de périphérique les connaît.
///
/// Cherchés parmi ce que le système dit de ses propres entrées, et non
/// parmi les périphériques d'une classe : un pavé de précision est un
/// appareil HID comme un autre, et seul ce qu'il déclare de son usage le
/// distingue d'un clavier ou d'une manette.
#[cfg(windows)]
fn the_precision_pads() -> Vec<String> {
    use windows_sys::Win32::UI::Input::{
        GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
        RIDI_DEVICEINFO, RIM_TYPEHID,
    };

    let mut how_many = 0u32;
    let each = std::mem::size_of::<RAWINPUTDEVICELIST>() as u32;
    // SAFETY: asked with no room at all, which is how this call is told
    // to answer with the count instead of the list.
    unsafe { GetRawInputDeviceList(std::ptr::null_mut(), &mut how_many, each) };
    if how_many == 0 {
        return Vec::new();
    }
    let mut devices: Vec<RAWINPUTDEVICELIST> =
        vec![unsafe { std::mem::zeroed() }; how_many as usize];
    // SAFETY: room of ours for exactly as many as were counted, and the
    // size of one is the one the call is told to expect.
    let written = unsafe { GetRawInputDeviceList(devices.as_mut_ptr(), &mut how_many, each) };
    if written == u32::MAX {
        return Vec::new();
    }
    devices.truncate(written as usize);

    let mut pads = Vec::new();
    for device in &devices {
        if device.dwType != RIM_TYPEHID {
            continue;
        }
        let mut about: RID_DEVICE_INFO = unsafe { std::mem::zeroed() };
        about.cbSize = std::mem::size_of::<RID_DEVICE_INFO>() as u32;
        let mut size = about.cbSize;
        // SAFETY: a handle the system has just named, and a description
        // of ours whose size it is told.
        let read = unsafe {
            GetRawInputDeviceInfoW(
                device.hDevice,
                RIDI_DEVICEINFO,
                std::ptr::from_mut(&mut about).cast(),
                &mut size,
            )
        };
        if read == u32::MAX {
            continue;
        }
        // SAFETY: the type was said to be HID just above, which is what
        // decides which of these describes the device.
        let hid = unsafe { about.Anonymous.hid };
        if (hid.usUsagePage, hid.usUsage) != A_TOUCHPAD {
            continue;
        }
        if let Some(named) = its_name(device.hDevice)
            && let Some(instance) = an_instance_of(&named)
        {
            pads.push(instance);
        }
    }
    pads.sort();
    pads.dedup();
    pads
}

/// The name the system gives that device, which is a path of its own.
#[cfg(windows)]
fn its_name(device: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
    use windows_sys::Win32::UI::Input::{GetRawInputDeviceInfoW, RIDI_DEVICENAME};

    let mut how_long = 0u32;
    // SAFETY: asked with no room, which answers with the length in
    // characters rather than writing anything.
    unsafe { GetRawInputDeviceInfoW(device, RIDI_DEVICENAME, std::ptr::null_mut(), &mut how_long) };
    if how_long == 0 {
        return None;
    }
    let mut name = vec![0u16; how_long as usize];
    // SAFETY: room of ours for exactly the length just given.
    let written = unsafe {
        GetRawInputDeviceInfoW(
            device,
            RIDI_DEVICENAME,
            name.as_mut_ptr().cast(),
            &mut how_long,
        )
    };
    if written == u32::MAX {
        return None;
    }
    let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
    Some(String::from_utf16_lossy(&name[..end]))
}

/// The same device, as an installer of devices names it.
///
/// The system writes one name as a path to open and the other as an
/// instance to act on, and they are the same thing spelled twice: the
/// opening one wears a prefix, swaps its separators and carries the class
/// it belongs to at the end. Written here rather than asked, because
/// there is nothing to ask: no call turns one into the other.
#[cfg(windows)]
fn an_instance_of(named: &str) -> Option<String> {
    let rest = named
        .strip_prefix("\\\\?\\")
        .or_else(|| named.strip_prefix("\\??\\"))?;
    // La classe est écrite au bout entre accolades, derrière le dernier
    // séparateur : un identifiant d'instance s'arrête avant elle.
    let rest = match rest.rfind("#{") {
        Some(brace) => &rest[..brace],
        None => rest,
    };
    if rest.is_empty() {
        return None;
    }
    Some(rest.replace('#', "\\"))
}

/// Éteint ce périphérique, puis le rallume.
///
/// Le rallumage est tenté même quand l'extinction a échoué, et son refus
/// prime sur tout le reste : un appareil laissé éteint est le seul
/// résultat que ce fichier n'a pas le droit de produire.
#[cfg(windows)]
fn turned_off_and_on(instance: &str) -> Result<(), String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DICS_DISABLE, DICS_ENABLE, SP_DEVINFO_DATA, SetupDiCreateDeviceInfoList,
        SetupDiDestroyDeviceInfoList, SetupDiOpenDeviceInfoW,
    };

    let wide: Vec<u16> = instance.encode_utf16().chain(Some(0)).collect();
    // SAFETY: no class asked for and no window named, which is how an
    // empty list is made.
    let list = unsafe { SetupDiCreateDeviceInfoList(std::ptr::null(), std::ptr::null_mut()) };
    // Ce refus-là ne se dit pas par zéro mais par la valeur que Windows
    // réserve aux poignées qui n'en sont pas, et une liste vide est une
    // poignée valable.
    if list == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE as isize {
        return Err("la liste des périphériques n'a pas pu être ouverte".to_string());
    }
    let mut about: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
    about.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
    // SAFETY: a list of ours, a name of ours ended by nought, and room of
    // ours whose size the call is told.
    let opened =
        unsafe { SetupDiOpenDeviceInfoW(list, wide.as_ptr(), std::ptr::null_mut(), 0, &mut about) };
    if opened == 0 {
        // SAFETY: the list this call made, given back on the spot.
        unsafe { SetupDiDestroyDeviceInfoList(list) };
        return Err(format!("introuvable : {}", std::io::Error::last_os_error()));
    }

    let off = told_to(list, &mut about, DICS_DISABLE);
    let on = told_to(list, &mut about, DICS_ENABLE);
    // SAFETY: the list this call made, given back once everything that
    // needed it is done.
    unsafe { SetupDiDestroyDeviceInfoList(list) };

    // Rallumé d'abord dans ce qui est dit : un pavé éteint qui ne
    // revient pas est un ordinateur sans pointeur, et c'est cela qu'il
    // faut lire en premier.
    on.map_err(|why| format!("rallumage refusé, le pavé est resté éteint : {why}"))?;
    off.map_err(|why| format!("extinction refusée : {why}"))
}

/// Dit à ce périphérique de s'éteindre ou de se rallumer.
#[cfg(windows)]
fn told_to(
    list: windows_sys::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
    about: &mut windows_sys::Win32::Devices::DeviceAndDriverInstallation::SP_DEVINFO_DATA,
    change: u32,
) -> Result<(), String> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DICS_FLAG_GLOBAL, DIF_PROPERTYCHANGE, SP_CLASSINSTALL_HEADER, SP_PROPCHANGE_PARAMS,
        SetupDiCallClassInstaller, SetupDiSetClassInstallParamsW,
    };

    let mut asked: SP_PROPCHANGE_PARAMS = unsafe { std::mem::zeroed() };
    asked.ClassInstallHeader.cbSize = std::mem::size_of::<SP_CLASSINSTALL_HEADER>() as u32;
    asked.ClassInstallHeader.InstallFunction = DIF_PROPERTYCHANGE;
    asked.StateChange = change;
    asked.Scope = DICS_FLAG_GLOBAL;
    asked.HwProfile = 0;

    // SAFETY: a list and a device of this call's own, and a description
    // of ours whose size the call is told.
    let set = unsafe {
        SetupDiSetClassInstallParamsW(
            list,
            about,
            std::ptr::from_ref(&asked).cast(),
            std::mem::size_of::<SP_PROPCHANGE_PARAMS>() as u32,
        )
    };
    if set == 0 {
        return Err(format!("{}", std::io::Error::last_os_error()));
    }
    // SAFETY: the same list and the same device, with the change just
    // written on them.
    let done = unsafe { SetupDiCallClassInstaller(DIF_PROPERTYCHANGE, list, about) };
    if done == 0 {
        return Err(format!("{}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    #[cfg(windows)]
    #[test]
    fn an_opening_name_becomes_an_instance_to_act_on() {
        // Le nom que le système donne à ouvrir, et celui sous lequel un
        // installeur le connaît : le même, écrit deux fois.
        assert_eq!(
            an_instance_of(
                "\\\\?\\HID#ELAN0670&Col01#5&37c1ba4b&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"
            )
            .as_deref(),
            Some("HID\\ELAN0670&Col01\\5&37c1ba4b&0&0000")
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_name_with_no_class_at_the_end_is_still_one() {
        assert_eq!(
            an_instance_of("\\\\?\\HID#ELAN0670&Col01#5&37c1ba4b&0&0000").as_deref(),
            Some("HID\\ELAN0670&Col01\\5&37c1ba4b&0&0000")
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_name_of_another_shape_is_refused_rather_than_guessed() {
        // Rien ici ne doit inventer un identifiant : ce qui en sort est
        // donné à un installeur de périphériques, qui éteint ce qu'on lui
        // nomme.
        assert!(an_instance_of("HID\\ELAN0670").is_none());
        assert!(an_instance_of("\\\\?\\").is_none());
    }
}
