//! Saying out loud that this driver's publisher is one we expect.
//!
//! Windows already trusts the driver carried here: it is signed by a
//! certificate that leads back, through an ordinary certificate
//! authority, to the roots Windows ships with. Nothing is added to that
//! chain and nothing needs to be, which is the whole reason this
//! particular driver was chosen.
//!
//! What Windows does not know is whether the person at this computer
//! *expects* a driver from that publisher. Not knowing, it asks, with a
//! window titled "would you like to install this device software?".
//! Nobody is there to answer: the installation runs from a service, on a
//! desktop with no one in front of it, and a question nobody answers is
//! an installation that fails.
//!
//! Naming the publisher as expected is what answers it in advance. It
//! grants nothing beyond that: an unsigned driver, or one signed by
//! somebody else, is refused exactly as before. It is taken back when
//! the product is removed.
//!
//! # Asking the file the right question
//!
//! A driver's catalogue is a signed message whose content happens to be
//! a list of file fingerprints. Asked what such a file is, without being
//! told what one is looking for, Windows answers with the list: it hands
//! back that list, and a store holding it, where a certificate was
//! expected and none is ever found. Every install on a machine that had
//! not been given this driver by other means therefore failed on
//! « carries no signature », over a file that carries three certificates
//! and that any other tool reads without trouble.
//!
//! So the question is asked the other way round: this file is a signed
//! message, hand over what signed it. Of the certificates a signature
//! carries, one is the publisher's and the others are the authorities
//! that vouch for it in turn; the message itself says which. Only that
//! one is taken. Naming an authority as expected would quietly extend
//! this machine's welcome to every driver that authority has ever signed,
//! which is a great many and none of them ours.

use std::ffi::{OsStr, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Security::Cryptography::{
    CERT_CONTEXT, CERT_FIND_EXISTING, CERT_FIND_SUBJECT_CERT, CERT_INFO,
    CERT_NAME_SIMPLE_DISPLAY_TYPE, CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED,
    CERT_QUERY_FORMAT_FLAG_BINARY, CERT_QUERY_OBJECT_FILE, CERT_STORE_ADD_REPLACE_EXISTING,
    CERT_STORE_PROV_SYSTEM_W, CERT_SYSTEM_STORE_LOCAL_MACHINE, CMSG_SIGNER_CERT_INFO_PARAM,
    CertAddCertificateContextToStore, CertCloseStore, CertDeleteCertificateFromStore,
    CertFindCertificateInStore, CertFreeCertificateContext, CertGetNameStringW, CertOpenStore,
    CryptMsgClose, CryptMsgGetParam, CryptQueryObject, HCERTSTORE, PKCS_7_ASN_ENCODING,
    X509_ASN_ENCODING,
};

use crate::{Done, Trouble};

/// Windows' own name for the list of publishers a machine expects.
const EXPECTED_PUBLISHERS: &str = "TrustedPublisher";

/// Names the publisher of `catalog` as one this computer expects.
pub fn vouch_for(catalog: &Path, done: &mut Done) -> Result<(), Trouble> {
    let publisher = Signature::on(catalog)?.publisher(catalog)?;
    let expected = Store::of_expected_publishers()?;
    // SAFETY: the store is open and the certificate is alive for the
    // whole call, which only copies it.
    let ok = unsafe {
        CertAddCertificateContextToStore(
            expected.0,
            publisher.0,
            CERT_STORE_ADD_REPLACE_EXISTING,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(crate::place::refused(
            "naming the virtual screen driver's publisher as expected",
        ));
    }
    done.step(format!(
        "virtual screen driver's publisher named as expected: {}",
        publisher.named()
    ));
    Ok(())
}

/// Takes that back, leaving the machine as it was found.
pub fn stop_vouching_for(catalog: &Path, done: &mut Done) -> Result<(), Trouble> {
    let publisher = Signature::on(catalog)?.publisher(catalog)?;
    let expected = Store::of_expected_publishers()?;
    // SAFETY: both stores are open, and the certificate handed over is
    // only read, to find its like in the other store.
    let found = unsafe {
        CertFindCertificateInStore(
            expected.0,
            X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
            0,
            CERT_FIND_EXISTING,
            publisher.0.cast::<c_void>(),
            std::ptr::null(),
        )
    };
    if found.is_null() {
        done.step(format!(
            "virtual screen driver's publisher was not named as expected ({}), nothing to take \
             back",
            publisher.named()
        ));
        return Ok(());
    }
    // SAFETY: a certificate the call above just handed us. Deleting it
    // also releases it, so it is never released twice.
    unsafe { CertDeleteCertificateFromStore(found) };
    done.step(format!(
        "virtual screen driver's publisher no longer named as expected: {}",
        publisher.named()
    ));
    Ok(())
}

/// The signature a file carries, opened for reading.
struct Signature {
    /// Every certificate the signature carries: the publisher's, and the
    /// authorities that vouch for it in turn.
    carried: Store,
    /// The signed message, which is what says which of them signed.
    message: Message,
}

impl Signature {
    fn on(file: &Path) -> Result<Self, Trouble> {
        let named = wide(file.as_os_str());
        let mut store: HCERTSTORE = std::ptr::null_mut();
        let mut message: *mut c_void = std::ptr::null_mut();
        // SAFETY: the name outlives the call, both slots for the answer
        // are ours, and every part of the answer we do not want is
        // refused by handing over nowhere to put it.
        let read = unsafe {
            CryptQueryObject(
                CERT_QUERY_OBJECT_FILE,
                named.as_ptr().cast::<c_void>(),
                CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED,
                CERT_QUERY_FORMAT_FLAG_BINARY,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut store,
                &mut message,
                std::ptr::null_mut(),
            )
        } != 0;
        // Wrapped before anything else can go wrong, so a question half
        // answered still closes what it opened.
        let carried = (!store.is_null()).then_some(Store(store));
        let message = (!message.is_null()).then_some(Message(message));
        match (read, carried, message) {
            (true, Some(carried), Some(message)) => Ok(Self { carried, message }),
            _ => Err(crate::place::refused(&format!(
                "reading the signature of {}",
                file.display()
            ))),
        }
    }

    /// The certificate of whoever signed the file.
    ///
    /// The message names it by its issuer and its serial number rather
    /// than handing it over, and the certificate itself is among the ones
    /// the signature carries: the name is read from the one and looked up
    /// in the other.
    fn publisher(&self, file: &Path) -> Result<Certificate, Trouble> {
        let named_by = self.message.signer(file)?;
        // SAFETY: the store is open, and what is handed over is a
        // description read out of the same signature, only read.
        let found = unsafe {
            CertFindCertificateInStore(
                self.carried.0,
                X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
                0,
                CERT_FIND_SUBJECT_CERT,
                named_by.as_ptr().cast::<c_void>(),
                std::ptr::null(),
            )
        };
        if found.is_null() {
            return Err(Trouble::PackageIncomplete {
                missing: format!("{} carries no signature", file.display()),
            });
        }
        Ok(Certificate(found))
    }
}

/// A signed message, closed once whatever happens.
struct Message(*const c_void);

impl Drop for Message {
    fn drop(&mut self) {
        // SAFETY: a message this file opened, closed exactly once.
        unsafe { CryptMsgClose(self.0) };
    }
}

impl Message {
    /// Who the message says signed it, as Windows describes a certificate
    /// it is not handing over: an issuer and a serial number.
    ///
    /// The answer is a structure followed by the pieces it points into,
    /// so it comes back as the block it was written in rather than as the
    /// structure alone. Counted in whole words and not in bytes: what is
    /// written there begins with a structure holding addresses, and an
    /// address read from an odd place is not the address that was
    /// written.
    fn signer(&self, file: &Path) -> Result<Vec<u64>, Trouble> {
        let mut size = 0u32;
        // SAFETY: the message is open, and asking with nowhere to write
        // is how this call is asked how much room it needs.
        let asked = unsafe {
            CryptMsgGetParam(
                self.0,
                CMSG_SIGNER_CERT_INFO_PARAM,
                0,
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if asked == 0 || (size as usize) < size_of::<CERT_INFO>() {
            return Err(Trouble::PackageIncomplete {
                missing: format!("{} carries no signature", file.display()),
            });
        }
        let mut block = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
        // SAFETY: the message is open and the block is at least as big as
        // the call has just said it needs.
        let written = unsafe {
            CryptMsgGetParam(
                self.0,
                CMSG_SIGNER_CERT_INFO_PARAM,
                0,
                block.as_mut_ptr().cast::<c_void>(),
                &mut size,
            )
        };
        if written == 0 {
            return Err(crate::place::refused(&format!(
                "reading who signed {}",
                file.display()
            )));
        }
        Ok(block)
    }
}

/// A certificate held for as long as it is needed, released once.
struct Certificate(*const CERT_CONTEXT);

impl Drop for Certificate {
    fn drop(&mut self) {
        // SAFETY: a certificate this file was handed and nothing else has
        // taken back.
        unsafe { CertFreeCertificateContext(self.0) };
    }
}

impl Certificate {
    /// The publisher's name as Windows shows it, for the journal.
    ///
    /// A machine being told to expect drivers from somebody should say in
    /// its journal from whom, and that name is the one thing anybody can
    /// check against what Windows shows beside the driver.
    fn named(&self) -> String {
        // SAFETY: our own certificate, and asking with nowhere to write
        // is how this call is asked how much room it needs.
        let room = unsafe {
            CertGetNameStringW(
                self.0,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            )
        };
        if room <= 1 {
            return "unnamed publisher".to_string();
        }
        let mut letters = vec![0u16; room as usize];
        // SAFETY: the same certificate, and the room is the one it has
        // just asked for.
        let written = unsafe {
            CertGetNameStringW(
                self.0,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null(),
                letters.as_mut_ptr(),
                room,
            )
        };
        // The count includes the nought the name ends on, which is not
        // part of the name.
        String::from_utf16_lossy(&letters[..(written.saturating_sub(1)) as usize])
    }
}

/// A certificate store, closed once whatever happens.
struct Store(HCERTSTORE);

impl Drop for Store {
    fn drop(&mut self) {
        // SAFETY: a store this file opened, closed exactly once.
        unsafe { CertCloseStore(self.0, 0) };
    }
}

impl Store {
    /// The machine's list of publishers it expects drivers from.
    fn of_expected_publishers() -> Result<Self, Trouble> {
        let named = wide(OsStr::new(EXPECTED_PUBLISHERS));
        // SAFETY: the provider is one of the numbers the call defines,
        // and the name outlives the call.
        let store = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_SYSTEM_W,
                0,
                0,
                CERT_SYSTEM_STORE_LOCAL_MACHINE,
                named.as_ptr().cast::<c_void>(),
            )
        };
        if store.is_null() {
            return Err(crate::place::refused(
                "opening this computer's list of expected publishers",
            ));
        }
        Ok(Self(store))
    }
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}
