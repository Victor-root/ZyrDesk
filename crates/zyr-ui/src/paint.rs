//! Ce qui dessine l'interface de ZyrDesk, sans navigateur.
//!
//! Une toile est un rectangle de pixels portant chacun sa transparence.
//! Remise à une fenêtre à calque, elle **est** la fenêtre : il n'y a ni
//! forme à découper, ni fond à effacer, ni cadre, et les clics passent
//! d'eux-mêmes partout où l'image est claire. C'est ce qui a réglé le
//! liseré du logo après douze essais.
//!
//! Une fenêtre ordinaire, elle, est encadrée par le système et opaque :
//! la toile s'y verse quand le système demande de repeindre. Les deux
//! dessinent de la même façon et n'en diffèrent qu'à la toute fin, `pose`
//! d'un côté et `verse` de l'autre.
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
//! le logo n'en emploie que le remplissage et le contour, le menu y
//! ajoute le texte, les icônes et les ombres, l'accueil le reste. Une
//! couche taillée sur le premier client se rouvre à chaque suivant, et
//! une couche qu'on rouvre est une couche dont personne ne connaît plus
//! les règles.
#![allow(dead_code)]

use windows::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F,
    D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_DASH, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_NONE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LINE_JOIN_ROUND,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory,
    ID2D1PathGeometry, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
    IDWriteTextLayout, IDWriteTextLayout1,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
    DeleteDC, DeleteObject, GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
use windows::core::{HSTRING, Interface};
use windows_numerics::{Matrix3x2, Vector2};

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

/// La famille à chasse fixe, et celle d'avant, dans le même ordre et pour
/// la même raison : ce que la feuille de style demande partout où des
/// signes doivent s'aligner les uns sous les autres.
const FIXE: &str = "Cascadia Mono";
const FIXE_AVANT: &str = "Consolas";

/// Un morceau d'icône, écrit dans les mêmes mots que le dessin dont il
/// vient.
pub enum Trait {
    /// Un « d » de chemin SVG, repris tel quel.
    ///
    /// Repris et non traduit : une icône transcrite à la main est une
    /// icône qui finit par ne plus être la même, et celles-ci sont déjà
    /// écrites une fois. Ce qui est compris ici est ce dont elles se
    /// servent : aller à, tracer jusqu'à, horizontalement, verticalement,
    /// une courbe, un arc, et refermer.
    Chemin(&'static str),
    /// Un rectangle arrondi : x, y, largeur, hauteur, rayon.
    Rond(f32, f32, f32, f32, f32),
}

/// Une icône : ses traits, le repère dans lequel ils sont écrits, et
/// l'épaisseur de son trait dans ce repère.
///
/// Elle porte son repère avec elle, comme le fait un dessin vectoriel :
/// c'est ce qui permet de la poser dans n'importe quel cadre sans que
/// personne ait à savoir en quelles unités elle a été dessinée.
pub struct Icone {
    pub repere: f32,
    pub epaisseur: f32,
    pub traits: &'static [Trait],
}

/// Où un mot se cale dans le cadre qu'on lui donne.
#[derive(Clone, Copy, PartialEq)]
pub enum Cale {
    Gauche,
    Centre,
    Droite,
}

/// Ce qu'un mot fait quand il ne tient pas dans son cadre.
#[derive(Clone, Copy, PartialEq)]
pub enum Trop {
    /// Il passe à la ligne, comme un paragraphe.
    ALaLigne,
    /// Il s'arrête sur des points de suspension, comme un nom d'ordinateur
    /// plus long que sa carte.
    Coupe,
    /// Il continue, et c'est au cadre de le retenir : une ligne de journal
    /// ne se replie pas, elle défile.
    Depasse,
}

/// Comment un mot s'écrit.
///
/// Tout ensemble parce que tout se décide ensemble : une mise en page de
/// texte se règle une fois pour toutes à sa fabrication, et la régler
/// après coup sur une police partagée change aussi ce que les **mesures**
/// emploient. Une plume est donc à la fois ce qu'on demande et la clé de
/// ce qui a déjà été fabriqué.
#[derive(Clone, Copy, PartialEq)]
pub struct Plume {
    pub taille: f32,
    pub gras: bool,
    pub cale: Cale,
    /// À chasse fixe : ce que la feuille de style demande pour une
    /// empreinte, un journal, un code et une combinaison de touches, où
    /// chaque signe doit tenir la place de son voisin.
    pub fixe: bool,
    pub trop: Trop,
    /// Ce qu'on ajoute entre deux signes, en vrais pixels.
    ///
    /// Ce que la feuille de style appelle `letter-spacing` : une étiquette
    /// de section en capitales et un code d'appairage se lisent mal
    /// resserrés, et c'est le seul endroit où l'espace entre les lettres
    /// est un choix du dessin.
    pub espace: f32,
}

impl Plume {
    /// Un mot ordinaire de cette taille, calé à gauche, qui passe à la
    /// ligne quand il ne tient pas.
    pub const fn de(taille: f32) -> Self {
        Plume {
            taille,
            gras: false,
            cale: Cale::Gauche,
            fixe: false,
            trop: Trop::ALaLigne,
            espace: 0.0,
        }
    }

    /// La même, les signes écartés d'autant de fois leur taille : c'est
    /// en `em` que la feuille de style l'écrit.
    pub fn ecartee(self, part: f32) -> Self {
        Plume {
            espace: self.taille * part,
            ..self
        }
    }

    pub const fn en_gras(self) -> Self {
        Plume { gras: true, ..self }
    }

    pub const fn a(self, cale: Cale) -> Self {
        Plume { cale, ..self }
    }

    pub const fn a_chasse_fixe(self) -> Self {
        Plume { fixe: true, ..self }
    }

    pub const fn coupee(self) -> Self {
        Plume {
            trop: Trop::Coupe,
            ..self
        }
    }

    pub const fn qui_depasse(self) -> Self {
        Plume {
            trop: Trop::Depasse,
            ..self
        }
    }
}

/// Une plume telle qu'on retrouve sa police : sa taille comptée au
/// millième de pixel, un nombre à virgule ne se comparant pas autrement
/// sans risquer de refabriquer la même police à chaque ligne.
///
/// L'écart entre les signes n'en fait pas partie, et ce n'est pas un
/// oubli : il se pose sur la mise en page d'un mot et non sur la police,
/// donc deux plumes qui ne diffèrent que par lui partagent la même.
#[derive(Clone, Copy, PartialEq)]
struct Clef {
    taille: u32,
    gras: bool,
    cale: Cale,
    fixe: bool,
    trop: Trop,
}

impl Clef {
    fn de(plume: Plume) -> Self {
        Clef {
            taille: (plume.taille * 1000.0).round() as u32,
            gras: plume.gras,
            cale: plume.cale,
            fixe: plume.fixe,
            trop: plume.trop,
        }
    }
}

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
    /// Les mises en page de texte déjà demandées, une par plume : les
    /// fabriquer coûte, s'en servir non, et un menu emploie deux tailles
    /// pour quinze lignes.
    polices: std::cell::RefCell<Vec<(Clef, IDWriteTextFormat)>>,
    /// Les chemins déjà lus, une fois chacun : une icône est un texte,
    /// et le relire à chaque image serait le relire quinze fois par
    /// dessin pour le même trait. Ceux qui ne se lisent pas sont retenus
    /// aussi, sans quoi leur refus se redirait à chaque image.
    chemins: std::cell::RefCell<Vec<(&'static str, Option<ID2D1PathGeometry>)>>,
    /// Le bout des traits et leurs angles, arrondis : c'est ce que les
    /// icônes demandent, et le demander une fois vaut mieux que le
    /// redemander à chaque trait.
    style: ID2D1StrokeStyle,
    /// Et le même en pointillés, pour ce qui attend d'être rempli.
    pointille: ID2D1StrokeStyle,
    fabrique: ID2D1Factory,
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
            let trait_ = |tirets| D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: tirets,
                dashOffset: 0.0,
            };
            let style = fabrique
                .CreateStrokeStyle(&trait_(D2D1_DASH_STYLE_SOLID), None)
                .ok()?;
            let pointille = fabrique
                .CreateStrokeStyle(&trait_(D2D1_DASH_STYLE_DASH), None)
                .ok()?;

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
                chemins: std::cell::RefCell::new(Vec::new()),
                style,
                pointille,
                fabrique,
            })
        }
    }

    /// Ce qu'elle fait de côté, pour qui doit savoir si elle est encore à
    /// la bonne taille.
    pub fn taille(&self) -> (i32, i32) {
        (self.large, self.haute)
    }

    /// Ouvre le dessin, la toile entièrement de cette couleur.
    ///
    /// `Couleur::RIEN` pour une fenêtre à calque, où la transparence
    /// laisse voir ce qu'il y a derrière ; un fond du système de design
    /// pour une fenêtre ordinaire, qui est opaque et n'a rien derrière.
    pub fn commence(&self, fond: Couleur) {
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
            self.cible.Clear(Some(&teinte(fond)));
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

    /// Verse la toile dans une surface, à cet endroit.
    ///
    /// Ce qu'il faut pour une fenêtre ordinaire, encadrée et opaque, qui
    /// se repeint quand le système le demande : `pose` remet l'image et
    /// la place en un seul geste, ce qu'une fenêtre à calque permet et
    /// qu'une fenêtre ordinaire ne connaît pas. La transparence ne
    /// voyage pas ici, et n'a rien à y faire : ce qu'on verse a été
    /// dessiné sur un fond.
    pub fn verse(&self, vers: HDC, x: i32, y: i32) -> bool {
        use windows::Win32::Graphics::Gdi::{BitBlt, SRCCOPY};

        // SAFETY: une surface à nous, recopiée telle quelle dans celle que
        // le système vient de prêter.
        unsafe {
            BitBlt(
                vers,
                x,
                y,
                self.large,
                self.haute,
                Some(self.surface),
                0,
                0,
                SRCCOPY,
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

    /// Le contour d'un rectangle arrondi, en pointillés.
    ///
    /// Ce que la feuille de style écrit `border-style: dashed`, et qui
    /// dit une seule chose dans tout le produit : ceci attend d'être
    /// rempli. Une carte pleine se borde d'un trait continu.
    pub fn trace_pointille(&self, cadre: Cadre, rayon: f32, epaisseur: f32, couleur: Couleur) {
        // SAFETY: comme au-dessus, avec le style pointillé fabriqué en
        // même temps que l'autre.
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
                &self.pointille,
            );
        }
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

    /// Dessine sans rien laisser sortir de ce cadre.
    ///
    /// Ce qu'il faut pour montrer une partie d'une forme sans en
    /// fabriquer une deuxième : les deux côtés d'un interrupteur sont un
    /// seul rectangle arrondi, et chacun n'en laisse voir que sa moitié.
    pub fn serre(&self, cadre: Cadre, dedans: impl FnOnce()) {
        // SAFETY: une cible à nous, entre un début et une fin de dessin,
        // dont la découpe est refermée avant de rendre la main.
        unsafe {
            self.cible
                .PushAxisAlignedClip(&cadre.dit(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
        dedans();
        // SAFETY: la découpe posée juste au-dessus.
        unsafe { self.cible.PopAxisAlignedClip() };
    }

    /// Écrit un mot dans ce cadre, calé comme la plume le dit et centré en
    /// hauteur.
    ///
    /// Centré en hauteur, donc un bloc replié veut un cadre de sa propre
    /// hauteur : `hauteur` la donne.
    pub fn ecris(&self, mot: &str, plume: Plume, couleur: Couleur, cadre: Cadre) {
        let Some(mise) = self.mise_en_page(
            mot,
            plume,
            cadre.droite - cadre.gauche,
            cadre.bas - cadre.haut,
        ) else {
            return;
        };
        // SAFETY: une mise en page à nous, employée le temps d'un dessin.
        unsafe {
            self.pinceau.SetColor(&teinte(couleur));
            self.cible.DrawTextLayout(
                vers((cadre.gauche, cadre.haut)),
                &mise,
                &self.pinceau,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
    }

    /// Ce qu'un mot prendrait de large, pour les endroits dont la largeur
    /// est celle de leur ligne la plus longue.
    pub fn largeur(&self, mot: &str, plume: Plume) -> f32 {
        self.mesure(mot, plume, AU_LARGE)
            .map_or(0.0, |mesure| mesure.widthIncludingTrailingWhitespace)
    }

    /// La hauteur qu'un mot prend, replié à cette largeur.
    ///
    /// Ce qu'il faut pour empiler des paragraphes : ce que chacun occupe
    /// dépend de la place qu'on lui laisse, et personne ne peut le deviner
    /// sans le mettre en page.
    pub fn hauteur(&self, mot: &str, plume: Plume, large: f32) -> f32 {
        self.mesure(mot, plume, large)
            .map_or(plume.taille, |mesure| mesure.height)
    }

    /// La hauteur d'une ligne de texte écrite de cette plume.
    ///
    /// Ce n'est pas la taille du caractère : une ligne de douze pixels en
    /// occupe environ seize, l'espace au-dessus et en dessous étant celui
    /// que la police elle-même demande. C'est cette hauteur-là qu'emploie
    /// la mise en page d'une page, et empiler du texte sur sa taille
    /// plutôt que sur sa hauteur serre tout ce qui est empilé.
    pub fn haute(&self, plume: Plume) -> f32 {
        // Deux lettres qui vont en haut et en bas : la hauteur d'une ligne
        // ne dépend pas de ce qu'on y écrit, mais une ligne vide n'en a
        // pas.
        self.hauteur("Hg", plume, AU_LARGE)
    }

    /// Ce qu'un mot mesure, mis en page hors de tout dessin.
    ///
    /// Dans une boîte large mais **finie** : mesurer dans une boîte
    /// démesurée fait perdre au calcul toute sa précision, et la largeur
    /// revient alors à rien du tout. C'est ce qui écrasait les
    /// interrupteurs du menu à la largeur de leur seule marge.
    fn mesure(&self, mot: &str, plume: Plume, large: f32) -> Option<DWRITE_TEXT_METRICS> {
        let mise = self.mise_en_page(mot, plume, large, AU_LARGE)?;
        // SAFETY: une mise en page à nous, mesurée et rendue aussitôt.
        unsafe {
            let mut mesure = DWRITE_TEXT_METRICS::default();
            mise.GetMetrics(&mut mesure).ok()?;
            Some(mesure)
        }
    }

    /// Un mot mis en page dans cette boîte, prêt à être mesuré ou
    /// dessiné.
    ///
    /// Le même chemin pour les deux, et c'est tout l'intérêt : ce qui est
    /// mesuré est exactement ce qui sera dessiné, écart entre les signes
    /// compris.
    fn mise_en_page(
        &self,
        mot: &str,
        plume: Plume,
        large: f32,
        haute: f32,
    ) -> Option<IDWriteTextLayout> {
        let police = self.police(plume)?;
        // SAFETY: une fabrique et une mise en page à nous.
        unsafe {
            let mise: IDWriteTextLayout = self
                .ecriture
                .CreateTextLayout(&lettres(mot), &police, large, haute)
                .ok()?;
            if plume.espace != 0.0 {
                // Derrière le mot et non devant : c'est ce que fait
                // `letter-spacing`, qui écarte les signes sans décaler le
                // premier de son bord.
                if let Ok(ecartee) = mise.cast::<IDWriteTextLayout1>() {
                    let _ = ecartee.SetCharacterSpacing(
                        0.0,
                        plume.espace,
                        0.0,
                        DWRITE_TEXT_RANGE {
                            startPosition: 0,
                            length: u32::MAX,
                        },
                    );
                }
            }
            Some(mise)
        }
    }

    /// La police de cette plume, fabriquée une fois.
    ///
    /// Toute la plume fait la clé, et ce n'est pas un détail : une mise en
    /// page se règle une fois pour toutes à sa fabrication. Réglée après
    /// coup sur une police partagée, elle change aussi celle que les
    /// **mesures** emploient, et une mesure prise dans une boîte alignée à
    /// droite ne vaut plus rien.
    fn police(&self, plume: Plume) -> Option<IDWriteTextFormat> {
        let clef = Clef::de(plume);
        if let Some((_, deja)) = self
            .polices
            .borrow()
            .iter()
            .find(|(autre, _)| *autre == clef)
        {
            return Some(deja.clone());
        }
        let neuve = self.fabrique_police(plume)?;
        self.polices.borrow_mut().push((clef, neuve.clone()));
        Some(neuve)
    }

    /// Demande la famille voulue, et celle d'avant si la machine n'a pas
    /// la première.
    fn fabrique_police(&self, plume: Plume) -> Option<IDWriteTextFormat> {
        let graisse = if plume.gras {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        let familles = if plume.fixe {
            [FIXE, FIXE_AVANT]
        } else {
            [FAMILLE, FAMILLE_AVANT]
        };
        // SAFETY: une fabrique à nous ; un refus est une réponse et non
        // une faute, d'où le second essai.
        unsafe {
            for famille in familles {
                let Ok(police) = self.ecriture.CreateTextFormat(
                    &HSTRING::from(famille),
                    None,
                    graisse,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    plume.taille,
                    &HSTRING::from("fr-FR"),
                ) else {
                    continue;
                };
                let _ = police.SetTextAlignment(match plume.cale {
                    Cale::Gauche => DWRITE_TEXT_ALIGNMENT_LEADING,
                    Cale::Centre => DWRITE_TEXT_ALIGNMENT_CENTER,
                    Cale::Droite => DWRITE_TEXT_ALIGNMENT_TRAILING,
                });
                let _ = police.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                self.pose_le_trop(&police, plume.trop);
                return Some(police);
            }
        }
        None
    }

    /// Règle ce que cette police fait d'un mot trop long.
    ///
    /// Les points de suspension sont un dessin, et un dessin se demande à
    /// la police qui le portera : c'est pour ça que ceci vient après elle
    /// et non avant.
    fn pose_le_trop(&self, police: &IDWriteTextFormat, trop: Trop) {
        if trop == Trop::ALaLigne {
            return;
        }
        // SAFETY: une police à nous, et une marque de coupe demandée à la
        // fabrique pour cette police-là.
        unsafe {
            let _ = police.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            if trop != Trop::Coupe {
                return;
            }
            let Ok(points) = self.ecriture.CreateEllipsisTrimmingSign(police) else {
                return;
            };
            let _ = police.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                },
                &points,
            );
        }
    }
}

/// Assez large pour qu'aucun mot n'aille à la ligne, et pas plus.
const AU_LARGE: f32 = 100_000.0;

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

impl Toile {
    /// Pose une icône dans ce cadre.
    ///
    /// L'icône est dessinée dans son propre repère et le cadre décide de
    /// sa taille : le trait suit, puisque le dessinateur met tout à
    /// l'échelle, y compris son épaisseur. C'est ce qui fait qu'une icône
    /// reste elle-même à cent vingt-cinq comme à cent soixante-quinze pour
    /// cent, là où une image agrandie s'épaissit et se brouille.
    pub fn icone(&self, icone: &Icone, cadre: Cadre, couleur: Couleur) {
        let part = (cadre.droite - cadre.gauche) / icone.repere;
        // SAFETY: une cible et un pinceau à nous, entre un début et une
        // fin de dessin. Le repère est remis d'aplomb avant de rendre la
        // main, sans quoi tout ce qui suivrait serait dessiné dans celui
        // de l'icône.
        unsafe {
            self.cible.SetTransform(&Matrix3x2 {
                M11: part,
                M12: 0.0,
                M21: 0.0,
                M22: part,
                M31: cadre.gauche,
                M32: cadre.haut,
            });
            self.pinceau.SetColor(&teinte(couleur));
            for trait_ in icone.traits {
                match trait_ {
                    Trait::Rond(x, y, large, haute, rayon) => self.cible.DrawRoundedRectangle(
                        &D2D1_ROUNDED_RECT {
                            rect: Cadre::pose(*x, *y, *large, *haute).dit(),
                            radiusX: *rayon,
                            radiusY: *rayon,
                        },
                        &self.pinceau,
                        icone.epaisseur,
                        &self.style,
                    ),
                    Trait::Chemin(dit) => {
                        if let Some(chemin) = self.chemin(dit) {
                            self.cible.DrawGeometry(
                                &chemin,
                                &self.pinceau,
                                icone.epaisseur,
                                &self.style,
                            );
                        }
                    }
                }
            }
            self.cible.SetTransform(&Matrix3x2 {
                M11: 1.0,
                M12: 0.0,
                M21: 0.0,
                M22: 1.0,
                M31: 0.0,
                M32: 0.0,
            });
        }
    }

    /// Le chemin de ce dessin, lu une fois.
    ///
    /// Un chemin illisible est retenu comme tel et dit une seule fois. Le
    /// retenir n'est pas de l'économie : sans ça il serait relu, et donc
    /// redit, à chaque image dessinée.
    fn chemin(&self, dit: &'static str) -> Option<ID2D1PathGeometry> {
        if let Some((_, deja)) = self
            .chemins
            .borrow()
            .iter()
            .find(|(autre, _)| std::ptr::eq(*autre, dit))
        {
            return deja.clone();
        }
        let neuf = self.lis(dit);
        if neuf.is_none() {
            // Dit et non tu. Une icône est faite de plusieurs traits :
            // celui qui ne se lit pas disparaît, les autres restent, et
            // ce qui s'affiche est une icône méconnaissable dont rien ne
            // dit qu'elle est incomplète. C'est arrivé une fois, à
            // l'oeil barré du menu, dont le contour est la seule courbe
            // de Bézier du produit.
            crate::journal::note(&format!("dessin : chemin non lu, « {dit} »"));
        }
        self.chemins.borrow_mut().push((dit, neuf.clone()));
        neuf
    }

    /// Lit un « d » de chemin SVG et en fait une forme.
    ///
    /// Ce qui est compris est ce dont les icônes de ce produit se
    /// servent, et rien de plus : aller à, tracer jusqu'à, tracer à
    /// l'horizontale, à la verticale, une courbe, un arc, et refermer.
    /// Une lettre inconnue arrête la lecture plutôt que d'être sautée :
    /// une icône à moitié dessinée ressemble à un défaut, une icône
    /// absente à un oubli, et le second se cherche. C'est `chemin` qui le
    /// dit à voix haute.
    fn lis(&self, dit: &str) -> Option<ID2D1PathGeometry> {
        // SAFETY: une forme et son embouchure à nous, refermées avant de
        // sortir.
        unsafe {
            let forme = self.fabrique.CreatePathGeometry().ok()?;
            let bouche = forme.Open().ok()?;
            let mut mots = Mots::sur(dit);
            let (mut ou, mut depart) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
            let mut ouverte = false;
            let mut lettre = ' ';
            while let Some(prochaine) = mots.lettre_ou_nombre() {
                if let Some(cette) = prochaine {
                    lettre = cette;
                }
                let relatif = lettre.is_lowercase();
                let mut nombre = || mots.nombre();
                match lettre.to_ascii_uppercase() {
                    'M' => {
                        let (x, y) = (nombre()?, nombre()?);
                        ou = if relatif {
                            (ou.0 + x, ou.1 + y)
                        } else {
                            (x, y)
                        };
                        if ouverte {
                            bouche.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        bouche.BeginFigure(vers(ou), D2D1_FIGURE_BEGIN_HOLLOW);
                        depart = ou;
                        ouverte = true;
                        lettre = if relatif { 'l' } else { 'L' };
                    }
                    'L' => {
                        let (x, y) = (nombre()?, nombre()?);
                        ou = if relatif {
                            (ou.0 + x, ou.1 + y)
                        } else {
                            (x, y)
                        };
                        bouche.AddLine(vers(ou));
                    }
                    'H' => {
                        let x = nombre()?;
                        ou.0 = if relatif { ou.0 + x } else { x };
                        bouche.AddLine(vers(ou));
                    }
                    'V' => {
                        let y = nombre()?;
                        ou.1 = if relatif { ou.1 + y } else { y };
                        bouche.AddLine(vers(ou));
                    }
                    'C' => {
                        // Les deux poignées se comptent depuis le point
                        // d'où la courbe part, donc avant de l'avoir
                        // quitté.
                        let (x1, y1) = (nombre()?, nombre()?);
                        let (x2, y2) = (nombre()?, nombre()?);
                        let (x, y) = (nombre()?, nombre()?);
                        let (une, deux) = if relatif {
                            ((ou.0 + x1, ou.1 + y1), (ou.0 + x2, ou.1 + y2))
                        } else {
                            ((x1, y1), (x2, y2))
                        };
                        ou = if relatif {
                            (ou.0 + x, ou.1 + y)
                        } else {
                            (x, y)
                        };
                        bouche.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: vers(une),
                            point2: vers(deux),
                            point3: vers(ou),
                        });
                    }
                    'A' => {
                        let (rx, ry) = (nombre()?, nombre()?);
                        let tourne = nombre()?;
                        let (grand, sens) = (nombre()?, nombre()?);
                        let (x, y) = (nombre()?, nombre()?);
                        ou = if relatif {
                            (ou.0 + x, ou.1 + y)
                        } else {
                            (x, y)
                        };
                        bouche.AddArc(&D2D1_ARC_SEGMENT {
                            point: vers(ou),
                            size: D2D_SIZE_F {
                                width: rx,
                                height: ry,
                            },
                            rotationAngle: tourne,
                            sweepDirection: if sens != 0.0 {
                                D2D1_SWEEP_DIRECTION_CLOCKWISE
                            } else {
                                D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                            },
                            arcSize: if grand != 0.0 {
                                D2D1_ARC_SIZE_LARGE
                            } else {
                                D2D1_ARC_SIZE_SMALL
                            },
                        });
                    }
                    'Z' => {
                        if ouverte {
                            bouche.EndFigure(D2D1_FIGURE_END_CLOSED);
                            ouverte = false;
                        }
                        ou = depart;
                    }
                    _ => return None,
                }
            }
            if ouverte {
                bouche.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            bouche.Close().ok()?;
            Some(forme)
        }
    }
}

/// Ce qu'un chemin SVG dit, lettre par lettre et nombre par nombre.
///
/// Un signe moins ouvre un nombre, il ne sépare pas : c'est la règle de
/// ce langage, et c'est ce qui permet d'écrire « a9 9 0 1 1-12.8 0 » sans
/// espace avant le douze.
struct Mots<'a> {
    reste: &'a str,
}

impl<'a> Mots<'a> {
    fn sur(dit: &'a str) -> Self {
        Mots { reste: dit }
    }

    fn saute(&mut self) {
        self.reste = self.reste.trim_start_matches([' ', ',', '\t', '\n']);
    }

    /// La prochaine chose : une lettre, ou rien quand c'est un nombre qui
    /// vient, ou la fin.
    fn lettre_ou_nombre(&mut self) -> Option<Option<char>> {
        self.saute();
        let premier = self.reste.chars().next()?;
        if premier.is_ascii_alphabetic() {
            self.reste = &self.reste[premier.len_utf8()..];
            return Some(Some(premier));
        }
        Some(None)
    }

    fn nombre(&mut self) -> Option<f32> {
        self.saute();
        let mut fin = 0;
        for (at, quoi) in self.reste.char_indices() {
            let ouvre = at == 0 && (quoi == '-' || quoi == '+');
            if quoi.is_ascii_digit() || quoi == '.' || ouvre {
                fin = at + quoi.len_utf8();
            } else {
                break;
            }
        }
        if fin == 0 {
            return None;
        }
        let (lu, reste) = self.reste.split_at(fin);
        self.reste = reste;
        lu.parse().ok()
    }
}

/// Un point, dans les nombres que le dessinateur attend.
fn vers(ou: (f32, f32)) -> Vector2 {
    Vector2 { X: ou.0, Y: ou.1 }
}

/// Un mot dans les caractères que Windows compte, qui ne sont pas ceux
/// de Rust.
fn lettres(mot: &str) -> Vec<u16> {
    mot.encode_utf16().collect()
}
