//! Ce qui dessine l'interface de ZyrDesk, sans navigateur.
//!
//! Une toile est un rectangle de pixels portant chacun sa transparence,
//! qu'on remet à Windows tel quel : la fenêtre **est** l'image. Il n'y a
//! donc ni forme à découper, ni fond à effacer, ni cadre, et les clics
//! passent d'eux-mêmes partout où l'image est claire. C'est ce qui a
//! réglé le liseré du logo après douze essais, et c'est pour ça que tout
//! le reste passera par ici.
//!
//! **Direct2D et DirectWrite**, fournis par Windows : rien n'est
//! embarqué, et le texte est rendu par le moteur qui rend celui du
//! système, donc il ressemble à celui du système.
//!
//! **Dessiné par le processeur, et c'est voulu.** La carte graphique
//! décode déjà de la vidéo en quatre mille par soixante ; lui demander en
//! plus de dessiner une carte serait ajouter un client à la file la plus
//! longue du produit. Une carte de menu coûte deux ou trois millisecondes
//! de processeur, et seulement quand quelque chose change : à l'ouverture,
//! au passage de la souris d'une ligne à l'autre, à la seconde qui fait
//! bouger les chiffres. Zéro le reste du temps.
//!
//! Les longueurs se comptent ici en **vrais pixels**, comme partout du
//! côté Rust. Ce que le système de design écrit est en pixels de page :
//! `echelle` fait le passage, une fois, à l'entrée.
//!
//! C'est une couche complète et non ce dont le premier écran a besoin :
//! le logo n'en emploie aujourd'hui que le remplissage et le contour, le
//! menu y ajoutera le texte et les ombres, et l'accueil le reste. Une
//! couche taillée sur le premier client se rouvre à chaque suivant, et
//! une couche qu'on rouvre est une couche dont personne ne connaît plus
//! les règles.
#![allow(dead_code)]

use windows::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
use windows::core::HSTRING;

use crate::design::{Couleur, Ombre};

/// La famille de caractères, celle du système, dans l'ordre où le
/// dessinateur la cherche.
///
/// La même que celle de la feuille de style, à la lettre près : deux
/// familles pour un produit, ce sont deux produits. Windows 11 a la
/// première, Windows 10 la seconde, et DirectWrite descend la liste tout
/// seul.
const FAMILLE: &str = "Segoe UI Variable Text";
const FAMILLE_AVANT: &str = "Segoe UI";

/// Un rectangle en vrais pixels, tel que tout ce fichier le compte.
#[derive(Clone, Copy)]
pub struct Cadre {
    pub gauche: f32,
    pub haut: f32,
    pub droite: f32,
    pub bas: f32,
}

impl Cadre {
    /// Le rectangle de coin haut gauche donné, de cette largeur et de
    /// cette hauteur.
    pub fn pose(gauche: f32, haut: f32, large: f32, haute: f32) -> Self {
        Cadre {
            gauche,
            haut,
            droite: gauche + large,
            bas: haut + haute,
        }
    }

    /// Le même, écarté de tous les côtés. Un écart négatif le resserre.
    pub fn elargi(&self, de: f32) -> Self {
        Cadre {
            gauche: self.gauche - de,
            haut: self.haut - de,
            droite: self.droite + de,
            bas: self.bas + de,
        }
    }

    /// Le même, décalé.
    pub fn decale(&self, de_x: f32, de_y: f32) -> Self {
        Cadre {
            gauche: self.gauche + de_x,
            haut: self.haut + de_y,
            droite: self.droite + de_x,
            bas: self.bas + de_y,
        }
    }

    fn dit(&self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.gauche,
            top: self.haut,
            right: self.droite,
            bottom: self.bas,
        }
    }
}

/// Une toile : des pixels, de quoi les dessiner, et de quoi les remettre
/// à une fenêtre.
///
/// Bâtie une fois par fenêtre et gardée : ce qui coûte ici est de la
/// bâtir, pas de dessiner dedans.
pub struct Toile {
    large: i32,
    haute: i32,
    surface: HDC,
    bitmap: HBITMAP,
    avant: HGDIOBJ,
    cible: ID2D1DCRenderTarget,
    pinceau: ID2D1SolidColorBrush,
    ecriture: IDWriteFactory,
    /// Les mises en page de texte déjà demandées, une par taille et par
    /// graisse : les fabriquer coûte, s'en servir non, et un menu emploie
    /// deux tailles pour quinze lignes.
    polices: std::cell::RefCell<Vec<(u32, bool, IDWriteTextFormat)>>,
    _fabrique: ID2D1Factory,
}

impl Toile {
    /// Une toile de cette taille, en vrais pixels.
    ///
    /// Rendue par le processeur et non par la carte graphique : voir le
    /// haut de ce fichier. C'est aussi ce qui évite d'avoir à survivre à
    /// la perte d'un appareil graphique, ce qui arrive précisément quand
    /// un pilote redémarre, c'est-à-dire au pire moment d'une session.
    pub fn neuve(large: i32, haute: i32) -> Option<Toile> {
        if large <= 0 || haute <= 0 {
            return None;
        }
        // SAFETY: chaque objet demandé au système est à nous jusqu'à ce
        // que `Drop` le rende, et rien n'en sort d'ici.
        unsafe {
            let ecran = GetDC(None);
            let surface = CreateCompatibleDC(Some(ecran));
            let mut carte: BITMAPINFO = std::mem::zeroed();
            carte.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: large,
                // À l'endroit, ce qui pour une image se dit d'une hauteur
                // négative.
                biHeight: -haute,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut pixels: *mut std::ffi::c_void = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(Some(surface), &carte, DIB_RGB_COLORS, &mut pixels, None, 0)
                    .ok()?;
            let avant = SelectObject(surface, bitmap.into());
            ReleaseDC(None, ecran);

            let fabrique: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let cible = fabrique
                .CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    // Tout est déjà compté en vrais pixels de ce côté-ci,
                    // donc on demande au dessinateur de ne rien
                    // remettre à l'échelle.
                    dpiX: 96.0,
                    dpiY: 96.0,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                })
                .ok()?;
            let pinceau = cible
                .CreateSolidColorBrush(&D2D1_COLOR_F::default(), None)
                .ok()?;
            let ecriture: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;

            Some(Toile {
                large,
                haute,
                surface,
                bitmap,
                avant,
                cible,
                pinceau,
                ecriture,
                polices: std::cell::RefCell::new(Vec::new()),
                _fabrique: fabrique,
            })
        }
    }

    /// Ouvre le dessin, la toile entièrement transparente.
    pub fn commence(&self) {
        let tout = RECT {
            left: 0,
            top: 0,
            right: self.large,
            bottom: self.haute,
        };
        // SAFETY: une cible et une surface à nous, liées le temps du
        // dessin comme la documentation le demande.
        unsafe {
            let _ = self.cible.BindDC(self.surface, &tout);
            self.cible.BeginDraw();
            self.cible.Clear(Some(&D2D1_COLOR_F::default()));
        }
    }

    /// Ferme le dessin, et dit si le dessinateur l'a accepté.
    pub fn finit(&self) -> bool {
        // SAFETY: la cible ouverte juste au-dessus.
        unsafe { self.cible.EndDraw(None, None).is_ok() }
    }

    /// Remet la toile à la fenêtre, image et transparence comprises, et
    /// la pose à cet endroit de l'écran.
    ///
    /// Un seul appel pour la place et pour l'image : la fenêtre ne peut
    /// donc pas être vue à son nouvel endroit avec son ancienne image.
    pub fn pose(&self, fenetre: isize, x: i32, y: i32) -> bool {
        let ou = POINT { x, y };
        let taille = SIZE {
            cx: self.large,
            cy: self.haute,
        };
        let depuis = POINT { x: 0, y: 0 };
        let melange = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
            BlendOp: windows::Win32::Graphics::Gdi::AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: windows::Win32::Graphics::Gdi::AC_SRC_ALPHA as u8,
        };
        // SAFETY: une fenêtre à nous et une surface à nous.
        unsafe {
            UpdateLayeredWindow(
                HWND(fenetre as *mut std::ffi::c_void),
                None,
                Some(&ou),
                Some(&taille),
                Some(self.surface),
                Some(&depuis),
                windows::Win32::Foundation::COLORREF(0),
                Some(&melange),
                ULW_ALPHA,
            )
            .is_ok()
        }
    }

    /// Un rectangle aux coins arrondis, rempli.
    pub fn remplis(&self, cadre: Cadre, rayon: f32, couleur: Couleur) {
        // SAFETY: un pinceau et une cible à nous, entre un début et une
        // fin de dessin.
        unsafe {
            self.pinceau.SetColor(&teinte(couleur));
            self.cible.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: cadre.dit(),
                    radiusX: rayon,
                    radiusY: rayon,
                },
                &self.pinceau,
            );
        }
    }

    /// Le contour d'un rectangle aux coins arrondis, tracé **à cheval**
    /// sur son bord : la moitié dedans, la moitié dehors.
    ///
    /// C'est ce que fait un trait dans un dessin vectoriel, donc c'est ce
    /// qu'il faut pour redessiner un dessin.
    pub fn trace_sur(&self, cadre: Cadre, rayon: f32, epaisseur: f32, couleur: Couleur) {
        // SAFETY: comme au-dessus.
        unsafe {
            self.pinceau.SetColor(&teinte(couleur));
            self.cible.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: cadre.dit(),
                    radiusX: rayon,
                    radiusY: rayon,
                },
                &self.pinceau,
                epaisseur,
                None,
            );
        }
    }

    /// Le même, mais tenant **entièrement à l'intérieur** du cadre.
    ///
    /// C'est ce que fait une bordure dans une page, donc c'est ce qu'il
    /// faut pour redessiner une interface que le système de design
    /// décrit. Les deux existent parce que les deux servent, et les
    /// confondre décale un bord d'un demi-trait.
    pub fn trace_dedans(&self, cadre: Cadre, rayon: f32, epaisseur: f32, couleur: Couleur) {
        self.trace_sur(
            cadre.elargi(-epaisseur / 2.0),
            (rayon - epaisseur / 2.0).max(0.0),
            epaisseur,
            couleur,
        );
    }

    /// L'ombre portée d'un rectangle arrondi, en vrais pixels.
    ///
    /// Faite de la silhouette redessinée en s'écartant, chacune très
    /// pâle, ce qui accumule une bordure douce du bord vers l'extérieur.
    /// Un flou gaussien demanderait un appareil graphique et ses
    /// tourments, pour une différence que personne ne voit sur une ombre
    /// de seize pixels posée sous une carte.
    pub fn ombre(&self, cadre: Cadre, rayon: f32, ombre: Ombre, echelle: f32) {
        let flou = ombre.soft * echelle;
        if flou <= 0.0 {
            return;
        }
        let pose = cadre.decale(ombre.across * echelle, ombre.down * echelle);
        let pas = flou.ceil().max(1.0) as i32;
        let mut voile = ombre.tint;
        voile.alpha = ombre.tint.alpha / pas as f32;
        for depuis in 0..pas {
            let ecart = flou * (1.0 - depuis as f32 / pas as f32);
            self.remplis(pose.elargi(ecart), rayon + ecart, voile);
        }
    }

    /// Écrit un mot dans ce cadre, aligné à gauche ou à droite et centré
    /// en hauteur, comme une ligne de menu l'attend.
    pub fn ecris(
        &self,
        mot: &str,
        taille: f32,
        gras: bool,
        couleur: Couleur,
        cadre: Cadre,
        a_droite: bool,
    ) {
        let Some(police) = self.police(taille, gras) else {
            return;
        };
        // SAFETY: une mise en page à nous, employée le temps d'un dessin.
        unsafe {
            let _ = police.SetTextAlignment(if a_droite {
                DWRITE_TEXT_ALIGNMENT_TRAILING
            } else {
                DWRITE_TEXT_ALIGNMENT_LEADING
            });
            let _ = police.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            self.pinceau.SetColor(&teinte(couleur));
            self.cible.DrawText(
                &lettres(mot),
                &police,
                &cadre.dit(),
                &self.pinceau,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
    }

    /// Ce qu'un mot prendrait de large, pour les endroits dont la largeur
    /// est celle de leur ligne la plus longue.
    pub fn largeur(&self, mot: &str, taille: f32, gras: bool) -> f32 {
        let Some(police) = self.police(taille, gras) else {
            return 0.0;
        };
        // SAFETY: une mise en page à nous, mesurée et rendue aussitôt.
        unsafe {
            let Ok(mise) = self.ecriture.CreateTextLayout(
                &lettres(mot),
                &police,
                f32::MAX / 2.0,
                f32::MAX / 2.0,
            ) else {
                return 0.0;
            };
            let mise: IDWriteTextLayout = mise;
            let mut mesure = DWRITE_TEXT_METRICS::default();
            if mise.GetMetrics(&mut mesure).is_err() {
                return 0.0;
            }
            mesure.widthIncludingTrailingWhitespace
        }
    }

    /// La police de cette taille et de cette graisse, fabriquée une fois.
    fn police(&self, taille: f32, gras: bool) -> Option<IDWriteTextFormat> {
        // Les tailles se comparent au millième de pixel pour servir de
        // clé : un nombre à virgule ne se compare pas autrement sans
        // risquer de refabriquer la même police à chaque ligne.
        let clef = (taille * 1000.0).round() as u32;
        if let Some((_, _, deja)) = self
            .polices
            .borrow()
            .iter()
            .find(|(autre, graisse, _)| *autre == clef && *graisse == gras)
        {
            return Some(deja.clone());
        }
        let graisse = if gras {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        let neuve = self.fabrique_police(taille, graisse)?;
        self.polices.borrow_mut().push((clef, gras, neuve.clone()));
        Some(neuve)
    }

    /// Demande la famille du système, et celle d'avant si la machine n'a
    /// pas la première.
    fn fabrique_police(
        &self,
        taille: f32,
        graisse: DWRITE_FONT_WEIGHT,
    ) -> Option<IDWriteTextFormat> {
        // SAFETY: une fabrique à nous ; un refus est une réponse et non
        // une faute, d'où le second essai.
        unsafe {
            for famille in [FAMILLE, FAMILLE_AVANT] {
                if let Ok(police) = self.ecriture.CreateTextFormat(
                    &HSTRING::from(famille),
                    None,
                    graisse,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    taille,
                    &HSTRING::from("fr-FR"),
                ) {
                    return Some(police);
                }
            }
        }
        None
    }
}

impl Drop for Toile {
    fn drop(&mut self) {
        // SAFETY: tout ce qui est rendu ici a été demandé dans `neuve`,
        // et dans l'ordre inverse.
        unsafe {
            let _ = SelectObject(self.surface, self.avant);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.surface);
        }
    }
}

/// Une couleur du système de design, dans les nombres que le dessinateur
/// attend.
fn teinte(couleur: Couleur) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: couleur.red,
        g: couleur.green,
        b: couleur.blue,
        a: couleur.alpha,
    }
}

/// Un mot dans les caractères que Windows compte, qui ne sont pas ceux
/// de Rust.
fn lettres(mot: &str) -> Vec<u16> {
    mot.encode_utf16().collect()
}
