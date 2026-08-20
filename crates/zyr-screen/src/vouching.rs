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

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Security::Cryptography::{
    CERT_CONTEXT, CERT_FIND_EXISTING, CERT_QUERY_CONTENT_FLAG_ALL, CERT_QUERY_FORMAT_FLAG_ALL,
    CERT_QUERY_OBJECT_FILE, CERT_STORE_ADD_REPLACE_EXISTING, CERT_STORE_PROV_SYSTEM_W,
    CERT_SYSTEM_STORE_LOCAL_MACHINE, CertAddCertificateContextToStore, CertCloseStore,
    CertDeleteCertificateFromStore, CertEnumCertificatesInStore, CertFindCertificateInStore,
    CertFreeCertificateContext, CertOpenStore, CryptQueryObject, HCERTSTORE, PKCS_7_ASN_ENCODING,
    X509_ASN_ENCODING,
};

use crate::{Done, Trouble};

/// Windows' own name for the list of publishers a machine expects.
const EXPECTED_PUBLISHERS: &str = "TrustedPublisher";

/// Names the publisher of `catalog` as one this computer expects.
pub fn vouch_for(catalog: &Path, done: &mut Done) -> Result<(), Trouble> {
    let signers = Store::of_the_signers(catalog)?;
    let expected = Store::of_expected_publishers()?;
    let mut added = 0;
    for signer in signers.certificates() {
        // SAFETY: both stores are open, and the certificate belongs to
        // the first one for as long as this loop runs.
        let ok = unsafe {
            CertAddCertificateContextToStore(
                expected.0,
                signer,
                CERT_STORE_ADD_REPLACE_EXISTING,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(crate::place::refused(
                "naming the virtual screen driver's publisher as expected",
            ));
        }
        added += 1;
    }
    if added == 0 {
        return Err(Trouble::PackageIncomplete {
            missing: format!("{} carries no signature", catalog.display()),
        });
    }
    done.step(format!(
        "virtual screen driver's publisher named as expected ({added} certificates from {})",
        catalog.display()
    ));
    Ok(())
}

/// Takes that back, leaving the machine as it was found.
pub fn stop_vouching_for(catalog: &Path, done: &mut Done) -> Result<(), Trouble> {
    let signers = Store::of_the_signers(catalog)?;
    let expected = Store::of_expected_publishers()?;
    let mut removed = 0;
    for signer in signers.certificates() {
        // SAFETY: both stores are open, and the certificate handed over
        // is only read, to find its like in the other store.
        let found = unsafe {
            CertFindCertificateInStore(
                expected.0,
                X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
                0,
                CERT_FIND_EXISTING,
                signer.cast::<std::ffi::c_void>(),
                std::ptr::null(),
            )
        };
        if found.is_null() {
            continue;
        }
        // SAFETY: a certificate the call above just handed us. Deleting
        // it also releases it, so it is never released twice.
        unsafe { CertDeleteCertificateFromStore(found) };
        removed += 1;
    }
    done.step(format!(
        "virtual screen driver's publisher no longer named as expected ({removed} certificates)"
    ));
    Ok(())
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
    /// The certificates that signed a file.
    fn of_the_signers(file: &Path) -> Result<Self, Trouble> {
        let named = wide(file.as_os_str());
        let mut store: HCERTSTORE = std::ptr::null_mut();
        // SAFETY: the name outlives the call, the slot for the answer is
        // ours, and every part of the answer we do not want is refused
        // by handing over nowhere to put it.
        let ok = unsafe {
            CryptQueryObject(
                CERT_QUERY_OBJECT_FILE,
                named.as_ptr().cast::<std::ffi::c_void>(),
                CERT_QUERY_CONTENT_FLAG_ALL,
                CERT_QUERY_FORMAT_FLAG_ALL,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut store,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || store.is_null() {
            return Err(crate::place::refused(&format!(
                "reading the signature of {}",
                file.display()
            )));
        }
        Ok(Self(store))
    }

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
                named.as_ptr().cast::<std::ffi::c_void>(),
            )
        };
        if store.is_null() {
            return Err(crate::place::refused(
                "opening this computer's list of expected publishers",
            ));
        }
        Ok(Self(store))
    }

    /// Every certificate the store holds.
    fn certificates(&self) -> Certificates<'_> {
        Certificates {
            store: self,
            at: std::ptr::null(),
        }
    }
}

/// Walks a store, releasing each certificate as it moves past it, which
/// is what the call itself does when handed the previous one.
struct Certificates<'a> {
    store: &'a Store,
    at: *const CERT_CONTEXT,
}

impl Iterator for Certificates<'_> {
    type Item = *const CERT_CONTEXT;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: the store is open, and what is handed over is either
        // nothing, to start, or the certificate this call gave last
        // time, which it takes back ownership of.
        let next = unsafe { CertEnumCertificatesInStore(self.store.0, self.at) };
        self.at = next;
        (!next.is_null()).then_some(next.cast_const())
    }
}

impl Drop for Certificates<'_> {
    fn drop(&mut self) {
        // A walk cut short leaves one certificate in hand, which the
        // call would have taken back had the walk gone to its end.
        if !self.at.is_null() {
            // SAFETY: a certificate the walk gave us and nothing has
            // taken back.
            unsafe { CertFreeCertificateContext(self.at) };
        }
    }
}

fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}
