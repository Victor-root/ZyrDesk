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
//! dessinent de la même façon et n'en diffèrent qu'à la toute fin, `shifted`
//! d'un côté et `copy_to` de l'autre.
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
//! `scale` fait le passage, une fois, à l'entrée.
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

use crate::design::{Colour, Shadow};

/// Ce sous quoi ce module classe ses lignes du journal.
const TAG: &str = "paint";

/// Écrit une ligne sous l'étiquette de ce module.
fn note(what: &str) {
    crate::journal::note_about(TAG, what);
}

/// La famille de caractères, celle du système, dans l'ordre où le
/// dessinateur la cherche.
///
/// La même que celle de la feuille de style, à la lettre près : deux
/// familles pour un produit, ce sont deux produits. Windows 11 a la
/// première, Windows 10 la seconde, et DirectWrite descend la liste tout
/// seul.
const FAMILY: &str = "Segoe UI Variable Text";
const FAMILY_BEFORE: &str = "Segoe UI";

/// La famille à chasse fixe, et celle d'avant, dans le même ordre et pour
/// la même raison : ce que la feuille de style demande partout où des
/// signes doivent s'aligner les uns sous les autres.
const MONO: &str = "Cascadia Mono";
const MONO_BEFORE: &str = "Consolas";

/// Un morceau d'icône, écrit dans les mêmes mots que le dessin dont il
/// vient.
pub enum Stroke {
    /// Un « d » de chemin SVG, repris tel quel.
    ///
    /// Repris et non traduit : une icône transcrite à la main est une
    /// icône qui finit par ne plus être la même, et celles-ci sont déjà
    /// écrites une fois. Ce qui est compris ici est ce dont elles se
    /// servent : aller à, tracer jusqu'à, horizontalement, verticalement,
    /// une courbe, un arc, et refermer.
    SvgPath(&'static str),
    /// Un rectangle arrondi : x, y, largeur, hauteur, rayon.
    RoundRect(f32, f32, f32, f32, f32),
}

/// Une icône : ses traits, le repère dans lequel ils sont écrits, et
/// l'épaisseur de son trait dans ce repère.
///
/// Elle porte son repère avec elle, comme le fait un dessin vectoriel :
/// c'est ce qui permet de la poser dans n'importe quel cadre sans que
/// personne ait à savoir en quelles unités elle a été dessinée.
pub struct Icon {
    pub grid: f32,
    pub thickness: f32,
    pub strokes: &'static [Stroke],
}

/// Où un mot se cale dans le cadre qu'on lui donne.
#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

/// Ce qu'un mot fait quand il ne tient pas dans son cadre.
#[derive(Clone, Copy, PartialEq)]
pub enum Overflow {
    /// Il passe à la ligne, comme un paragraphe.
    Wrap,
    /// Il s'arrête sur des points de suspension, comme un nom d'ordinateur
    /// plus long que sa carte.
    Ellipsis,
    /// Il continue, et c'est au cadre de le retenir : une ligne de journal
    /// ne se replie pas, elle défile.
    Visible,
}

/// Comment un mot s'écrit.
///
/// Tout ensemble parce que tout se décide ensemble : une mise en page de
/// texte se règle une fois pour toutes à sa fabrication, et la régler
/// après coup sur une police partagée change aussi ce que les **mesures**
/// emploient. Une plume est donc à la fois ce qu'on demande et la clé de
/// ce qui a déjà été fabriqué.
#[derive(Clone, Copy, PartialEq)]
pub struct Pen {
    pub size: f32,
    pub bold: bool,
    pub align: Align,
    /// À chasse fixe : ce que la feuille de style demande pour une
    /// empreinte, un journal, un code et une combinaison de touches, où
    /// chaque signe doit tenir la place de son voisin.
    pub mono: bool,
    pub overflow: Overflow,
    /// Ce qu'on ajoute entre deux signes, en vrais pixels.
    ///
    /// Ce que la feuille de style appelle `letter-spacing` : une étiquette
    /// de section en capitales et un code d'appairage se lisent mal
    /// resserrés, et c'est le seul endroit où l'espace entre les lettres
    /// est un choix du dessin.
    pub spacing: f32,
}

impl Pen {
    /// Un mot ordinaire de cette taille, calé à gauche, qui passe à la
    /// ligne quand il ne tient pas.
    pub const fn of(size: f32) -> Self {
        Pen {
            size,
            bold: false,
            align: Align::Left,
            mono: false,
            overflow: Overflow::Wrap,
            spacing: 0.0,
        }
    }

    /// La même, les signes écartés d'autant de fois leur taille : c'est
    /// en `em` que la feuille de style l'écrit.
    pub fn spaced(self, part: f32) -> Self {
        Pen {
            spacing: self.size * part,
            ..self
        }
    }

    pub const fn in_bold(self) -> Self {
        Pen { bold: true, ..self }
    }

    pub const fn aligned(self, align: Align) -> Self {
        Pen { align, ..self }
    }

    pub const fn monospaced(self) -> Self {
        Pen { mono: true, ..self }
    }

    pub const fn ellipsized(self) -> Self {
        Pen {
            overflow: Overflow::Ellipsis,
            ..self
        }
    }

    pub const fn overflowing(self) -> Self {
        Pen {
            overflow: Overflow::Visible,
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
struct Key {
    size: u32,
    bold: bool,
    align: Align,
    mono: bool,
    overflow: Overflow,
}

impl Key {
    fn of(pen: Pen) -> Self {
        Key {
            size: (pen.size * 1000.0).round() as u32,
            bold: pen.bold,
            align: pen.align,
            mono: pen.mono,
            overflow: pen.overflow,
        }
    }
}

/// Un rectangle en vrais pixels, tel que tout ce fichier le compte.
#[derive(Clone, Copy)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    /// Le rectangle de coin haut gauche donné, de cette largeur et de
    /// cette hauteur.
    pub fn at(left: f32, top: f32, width: f32, height: f32) -> Self {
        Rect {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }

    /// Le même, écarté de tous les côtés. Un écart négatif le resserre.
    pub fn grown(&self, by: f32) -> Self {
        Rect {
            left: self.left - by,
            top: self.top - by,
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }

    /// Le même, décalé.
    pub fn shifted(&self, dx: f32, dy: f32) -> Self {
        Rect {
            left: self.left + dx,
            top: self.top + dy,
            right: self.right + dx,
            bottom: self.bottom + dy,
        }
    }

    fn d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }
}

/// Une toile : des pixels, de quoi les dessiner, et de quoi les remettre
/// à une fenêtre.
///
/// Bâtie une fois par fenêtre et gardée : ce qui coûte ici est de la
/// bâtir, pas de dessiner dedans.
pub struct Canvas {
    width: i32,
    height: i32,
    surface: HDC,
    bitmap: HBITMAP,
    before: HGDIOBJ,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    writer: IDWriteFactory,
    /// Les mises en page de texte déjà demandées, une par plume : les
    /// fabriquer coûte, s'en servir non, et un menu emploie deux tailles
    /// pour quinze lignes.
    fonts: std::cell::RefCell<Vec<(Key, IDWriteTextFormat)>>,
    /// Les chemins déjà lus, une fois chacun : une icône est un texte,
    /// et le relire à chaque image serait le relire quinze fois par
    /// dessin pour le même trait. Ceux qui ne se lisent pas sont retenus
    /// aussi, sans quoi leur refus se redirait à chaque image.
    paths: std::cell::RefCell<Vec<(&'static str, Option<ID2D1PathGeometry>)>>,
    /// Le bout des traits et leurs angles, arrondis : c'est ce que les
    /// icônes demandent, et le demander une fois vaut mieux que le
    /// redemander à chaque trait.
    style: ID2D1StrokeStyle,
    /// Et le même en pointillés, pour ce qui attend d'être rempli.
    dashed: ID2D1StrokeStyle,
    factory: ID2D1Factory,
}

impl Canvas {
    /// Une toile de cette taille, en vrais pixels.
    ///
    /// Rendue par le processeur et non par la carte graphique : voir le
    /// haut de ce fichier. C'est aussi ce qui évite d'avoir à survivre à
    /// la perte d'un appareil graphique, ce qui arrive précisément quand
    /// un pilote redémarre, c'est-à-dire au pire moment d'une session.
    pub fn new(width: i32, height: i32) -> Option<Canvas> {
        if width <= 0 || height <= 0 {
            return None;
        }
        // SAFETY: chaque objet demandé au système est à nous jusqu'à ce
        // que `Drop` le rende, et rien n'en sort d'ici.
        unsafe {
            let screen = GetDC(None);
            let surface = CreateCompatibleDC(Some(screen));
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // À l'endroit, ce qui pour une image se dit d'une hauteur
                // négative.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut pixels: *mut std::ffi::c_void = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(Some(surface), &info, DIB_RGB_COLORS, &mut pixels, None, 0)
                    .ok()?;
            let before = SelectObject(surface, bitmap.into());
            ReleaseDC(None, screen);

            let factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()?;
            let target = factory
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
            let brush = target
                .CreateSolidColorBrush(&D2D1_COLOR_F::default(), None)
                .ok()?;
            let writer: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
            let stroke = |dashes| D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: dashes,
                dashOffset: 0.0,
            };
            let style = factory
                .CreateStrokeStyle(&stroke(D2D1_DASH_STYLE_SOLID), None)
                .ok()?;
            let dashed = factory
                .CreateStrokeStyle(&stroke(D2D1_DASH_STYLE_DASH), None)
                .ok()?;

            Some(Canvas {
                width,
                height,
                surface,
                bitmap,
                before,
                target,
                brush,
                writer,
                fonts: std::cell::RefCell::new(Vec::new()),
                paths: std::cell::RefCell::new(Vec::new()),
                style,
                dashed,
                factory,
            })
        }
    }

    /// Ce qu'elle fait de côté, pour qui doit savoir si elle est encore à
    /// la bonne taille.
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Ouvre le dessin, la toile entièrement de cette couleur.
    ///
    /// `Colour::TRANSPARENT` pour une fenêtre à calque, où la transparence
    /// laisse voir ce qu'il y a derrière ; un fond du système de design
    /// pour une fenêtre ordinaire, qui est opaque et n'a rien derrière.
    pub fn begin(&self, background: Colour) {
        let whole = RECT {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
        // SAFETY: une cible et une surface à nous, liées le temps du
        // dessin comme la documentation le demande.
        unsafe {
            let _ = self.target.BindDC(self.surface, &whole);
            self.target.BeginDraw();
            self.target.Clear(Some(&tint(background)));
        }
    }

    /// Ferme le dessin, et dit si le dessinateur l'a accepté.
    pub fn finish(&self) -> bool {
        // SAFETY: la cible ouverte juste au-dessus.
        unsafe { self.target.EndDraw(None, None).is_ok() }
    }

    /// Remet la toile à la fenêtre, image et transparence comprises, et
    /// la pose à cet endroit de l'écran.
    ///
    /// Un seul appel pour la place et pour l'image : la fenêtre ne peut
    /// donc pas être vue à son nouvel endroit avec son ancienne image.
    pub fn lay_on(&self, window: isize, x: i32, y: i32) -> bool {
        let at = POINT { x, y };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = windows::Win32::Graphics::Gdi::BLENDFUNCTION {
            BlendOp: windows::Win32::Graphics::Gdi::AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: windows::Win32::Graphics::Gdi::AC_SRC_ALPHA as u8,
        };
        // SAFETY: une fenêtre à nous et une surface à nous.
        unsafe {
            UpdateLayeredWindow(
                HWND(window as *mut std::ffi::c_void),
                None,
                Some(&at),
                Some(&size),
                Some(self.surface),
                Some(&source),
                windows::Win32::Foundation::COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
            .is_ok()
        }
    }

    /// Verse la toile dans une surface, à cet endroit.
    ///
    /// Ce qu'il faut pour une fenêtre ordinaire, encadrée et opaque, qui
    /// se repeint quand le système le demande : `shifted` remet l'image et
    /// la place en un seul geste, ce qu'une fenêtre à calque permet et
    /// qu'une fenêtre ordinaire ne connaît pas. La transparence ne
    /// voyage pas ici, et n'a rien à y faire : ce qu'on verse a été
    /// dessiné sur un fond.
    pub fn copy_to(&self, dc: HDC, x: i32, y: i32) -> bool {
        use windows::Win32::Graphics::Gdi::{BitBlt, SRCCOPY};

        // SAFETY: une surface à nous, recopiée telle quelle dans celle que
        // le système vient de prêter.
        unsafe {
            BitBlt(
                dc,
                x,
                y,
                self.width,
                self.height,
                Some(self.surface),
                0,
                0,
                SRCCOPY,
            )
            .is_ok()
        }
    }

    /// Fait de la toile une icône du système.
    ///
    /// Ce qu'il faut pour la zone de notification, qui ne prend pas une
    /// image mais une icône. Le système en garde une copie, donc la toile
    /// reste à nous ; ce qui revient est à celui qui le demande, jusqu'à
    /// ce qu'il le rende.
    ///
    /// Le masque est celui d'une icône en quatre octets par pixel : tout
    /// à zéro, la transparence étant portée par les pixels eux-mêmes.
    pub fn to_icon(&self) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
        use windows::Win32::Graphics::Gdi::CreateBitmap;
        use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};

        // Une ligne d'un dessin à un bit par pixel est comptée en mots de
        // seize bits, ce que la taille ci-dessous arrondit.
        let per_row = ((self.width as usize).div_ceil(16)) * 2;
        let blank = vec![0u8; per_row * self.height.max(0) as usize];
        // SAFETY: un dessin fait ici et rendu ici, et une icône que le
        // système recopie avant de rendre la main.
        unsafe {
            let mask = CreateBitmap(
                self.width,
                self.height,
                1,
                1,
                Some(blank.as_ptr().cast::<std::ffi::c_void>()),
            );
            if mask.is_invalid() {
                return None;
            }
            let icon = CreateIconIndirect(&ICONINFO {
                fIcon: true.into(),
                xHotspot: 0,
                yHotspot: 0,
                hbmMask: mask,
                hbmColor: self.bitmap,
            });
            let _ = DeleteObject(mask.into());
            icon.ok()
        }
    }

    /// Un rectangle aux coins arrondis, rempli.
    pub fn fill(&self, rect: Rect, radius: f32, colour: Colour) {
        // SAFETY: un pinceau et une cible à nous, entre un début et une
        // fin de dessin.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
            );
        }
    }

    /// Le contour d'un rectangle aux coins arrondis, tracé **à cheval**
    /// sur son bord : la moitié dedans, la moitié dehors.
    ///
    /// C'est ce que fait un trait dans un dessin vectoriel, donc c'est ce
    /// qu'il faut pour redessiner un dessin.
    pub fn stroke_on(&self, rect: Rect, radius: f32, thickness: f32, colour: Colour) {
        // SAFETY: comme au-dessus.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                thickness,
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
    pub fn stroke_inside(&self, rect: Rect, radius: f32, thickness: f32, colour: Colour) {
        self.stroke_on(
            rect.grown(-thickness / 2.0),
            (radius - thickness / 2.0).max(0.0),
            thickness,
            colour,
        );
    }

    /// Le contour d'un rectangle arrondi, en pointillés.
    ///
    /// Ce que la feuille de style écrit `border-style: dashed`, et qui
    /// dit une seule chose dans tout le produit : ceci attend d'être
    /// rempli. Une carte pleine se borde d'un trait continu.
    pub fn stroke_dashed(&self, rect: Rect, radius: f32, thickness: f32, colour: Colour) {
        // SAFETY: comme au-dessus, avec le style pointillé fabriqué en
        // même temps que l'autre.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect.d2d(),
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                thickness,
                &self.dashed,
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
    pub fn shadow(&self, rect: Rect, radius: f32, shadow: Shadow, scale: f32) {
        let blur = shadow.soft * scale;
        if blur <= 0.0 {
            return;
        }
        let shifted = rect.shifted(shadow.across * scale, shadow.down * scale);
        let steps = blur.ceil().max(1.0) as i32;
        let mut tint = shadow.tint;
        tint.alpha = shadow.tint.alpha / steps as f32;
        for step in 0..steps {
            let gap = blur * (1.0 - step as f32 / steps as f32);
            self.fill(shifted.grown(gap), radius + gap, tint);
        }
    }

    /// Dessine sans rien laisser sortir de ce cadre.
    ///
    /// Ce qu'il faut pour montrer une partie d'une forme sans en
    /// fabriquer une deuxième : les deux côtés d'un interrupteur sont un
    /// seul rectangle arrondi, et chacun n'en laisse voir que sa moitié.
    pub fn clipped(&self, rect: Rect, inside: impl FnOnce()) {
        // SAFETY: une cible à nous, entre un début et une fin de dessin,
        // dont la découpe est refermée avant de rendre la main.
        unsafe {
            self.target
                .PushAxisAlignedClip(&rect.d2d(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
        inside();
        // SAFETY: la découpe posée juste au-dessus.
        unsafe { self.target.PopAxisAlignedClip() };
    }

    /// Écrit un mot dans ce cadre, calé comme la plume le dit et centré en
    /// hauteur.
    ///
    /// Centré en hauteur, donc un bloc replié veut un cadre de sa propre
    /// hauteur : `height` la donne.
    pub fn draw_text(&self, text: &str, pen: Pen, colour: Colour, rect: Rect) {
        let Some(layout) =
            self.text_layout(text, pen, rect.right - rect.left, rect.bottom - rect.top)
        else {
            return;
        };
        // SAFETY: une mise en page à nous, employée le temps d'un dessin.
        unsafe {
            self.brush.SetColor(&tint(colour));
            self.target.DrawTextLayout(
                point((rect.left, rect.top)),
                &layout,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
        }
    }

    /// Ce qu'un mot prendrait de large, pour les endroits dont la largeur
    /// est celle de leur ligne la plus longue.
    pub fn width_of(&self, text: &str, pen: Pen) -> f32 {
        self.measure(text, pen, UNBOUNDED)
            .map_or(0.0, |measure| measure.widthIncludingTrailingWhitespace)
    }

    /// La hauteur qu'un mot prend, replié à cette largeur.
    ///
    /// Ce qu'il faut pour empiler des paragraphes : ce que chacun occupe
    /// dépend de la place qu'on lui laisse, et personne ne peut le deviner
    /// sans le mettre en page.
    pub fn height_of(&self, text: &str, pen: Pen, width: f32) -> f32 {
        self.measure(text, pen, width)
            .map_or(pen.size, |measure| measure.height)
    }

    /// La hauteur d'une ligne de texte écrite de cette plume.
    ///
    /// Ce n'est pas la taille du caractère : une ligne de douze pixels en
    /// occupe environ seize, l'espace au-dessus et en dessous étant celui
    /// que la police elle-même demande. C'est cette hauteur-là qu'emploie
    /// la mise en page d'une page, et empiler du texte sur sa taille
    /// plutôt que sur sa hauteur serre tout ce qui est empilé.
    pub fn line_height(&self, pen: Pen) -> f32 {
        // Deux lettres qui vont en haut et en bas : la hauteur d'une ligne
        // ne dépend pas de ce qu'on y écrit, mais une ligne vide n'en a
        // pas.
        self.height_of("Hg", pen, UNBOUNDED)
    }

    /// Ce qu'un mot mesure, mis en page hors de tout dessin.
    ///
    /// Dans une boîte large mais **finie** : mesurer dans une boîte
    /// démesurée fait perdre au calcul toute sa précision, et la largeur
    /// revient alors à rien du tout. C'est ce qui écrasait les
    /// interrupteurs du menu à la largeur de leur seule marge.
    fn measure(&self, text: &str, pen: Pen, width: f32) -> Option<DWRITE_TEXT_METRICS> {
        let layout = self.text_layout(text, pen, width, UNBOUNDED)?;
        // SAFETY: une mise en page à nous, mesurée et rendue aussitôt.
        unsafe {
            let mut measure = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut measure).ok()?;
            Some(measure)
        }
    }

    /// Un mot mis en page dans cette boîte, prêt à être mesuré ou
    /// dessiné.
    ///
    /// Le même chemin pour les deux, et c'est tout l'intérêt : ce qui est
    /// mesuré est exactement ce qui sera dessiné, écart entre les signes
    /// compris.
    fn text_layout(
        &self,
        text: &str,
        pen: Pen,
        width: f32,
        height: f32,
    ) -> Option<IDWriteTextLayout> {
        let font = self.font(pen)?;
        // SAFETY: une fabrique et une mise en page à nous.
        unsafe {
            let layout: IDWriteTextLayout = self
                .writer
                .CreateTextLayout(&utf16(text), &font, width, height)
                .ok()?;
            if pen.spacing != 0.0 {
                // Derrière le mot et non devant : c'est ce que fait
                // `letter-spacing`, qui écarte les signes sans décaler le
                // premier de son bord.
                if let Ok(spaced) = layout.cast::<IDWriteTextLayout1>() {
                    let _ = spaced.SetCharacterSpacing(
                        0.0,
                        pen.spacing,
                        0.0,
                        DWRITE_TEXT_RANGE {
                            startPosition: 0,
                            length: u32::MAX,
                        },
                    );
                }
            }
            Some(layout)
        }
    }

    /// La police de cette plume, fabriquée une fois.
    ///
    /// Toute la plume fait la clé, et ce n'est pas un détail : une mise en
    /// page se règle une fois pour toutes à sa fabrication. Réglée après
    /// coup sur une police partagée, elle change aussi celle que les
    /// **mesures** emploient, et une mesure prise dans une boîte alignée à
    /// droite ne vaut plus rien.
    fn font(&self, pen: Pen) -> Option<IDWriteTextFormat> {
        let key = Key::of(pen);
        if let Some((_, found)) = self.fonts.borrow().iter().find(|(other, _)| *other == key) {
            return Some(found.clone());
        }
        let made = self.make_font(pen)?;
        self.fonts.borrow_mut().push((key, made.clone()));
        Some(made)
    }

    /// Demande la famille voulue, et celle d'avant si la machine n'a pas
    /// la première.
    fn make_font(&self, pen: Pen) -> Option<IDWriteTextFormat> {
        let weight = if pen.bold {
            DWRITE_FONT_WEIGHT_SEMI_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        };
        let families = if pen.mono {
            [MONO, MONO_BEFORE]
        } else {
            [FAMILY, FAMILY_BEFORE]
        };
        // SAFETY: une fabrique à nous ; un refus est une réponse et non
        // une faute, d'où le second essai.
        unsafe {
            for family in families {
                let Ok(font) = self.writer.CreateTextFormat(
                    &HSTRING::from(family),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    pen.size,
                    &HSTRING::from("fr-FR"),
                ) else {
                    continue;
                };
                let _ = font.SetTextAlignment(match pen.align {
                    Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                    Align::Centre => DWRITE_TEXT_ALIGNMENT_CENTER,
                    Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
                });
                let _ = font.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
                self.set_the_overflow(&font, pen.overflow);
                return Some(font);
            }
        }
        None
    }

    /// Règle ce que cette police fait d'un mot trop long.
    ///
    /// Les points de suspension sont un dessin, et un dessin se demande à
    /// la police qui le portera : c'est pour ça que ceci vient après elle
    /// et non avant.
    fn set_the_overflow(&self, font: &IDWriteTextFormat, overflow: Overflow) {
        if overflow == Overflow::Wrap {
            return;
        }
        // SAFETY: une police à nous, et une marque de coupe demandée à la
        // fabrique pour cette police-là.
        unsafe {
            let _ = font.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            if overflow != Overflow::Ellipsis {
                return;
            }
            let Ok(points) = self.writer.CreateEllipsisTrimmingSign(font) else {
                return;
            };
            let _ = font.SetTrimming(
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
const UNBOUNDED: f32 = 100_000.0;

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: tout ce qui est rendu ici a été demandé dans `new`,
        // et dans l'ordre inverse.
        unsafe {
            let _ = SelectObject(self.surface, self.before);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.surface);
        }
    }
}

/// Une couleur du système de design, dans les nombres que le dessinateur
/// attend.
fn tint(colour: Colour) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: colour.red,
        g: colour.green,
        b: colour.blue,
        a: colour.alpha,
    }
}

impl Canvas {
    /// Pose une icône dans ce cadre.
    ///
    /// L'icône est dessinée dans son propre repère et le cadre décide de
    /// sa taille : le trait suit, puisque le dessinateur met tout à
    /// l'échelle, y compris son épaisseur. C'est ce qui fait qu'une icône
    /// reste elle-même à cent vingt-cinq comme à cent soixante-quinze pour
    /// cent, là où une image agrandie s'épaissit et se brouille.
    pub fn icon(&self, icon: &Icon, rect: Rect, colour: Colour) {
        let part = (rect.right - rect.left) / icon.grid;
        // SAFETY: une cible et un pinceau à nous, entre un début et une
        // fin de dessin. Le repère est remis d'aplomb avant de rendre la
        // main, sans quoi tout ce qui suivrait serait dessiné dans celui
        // de l'icône.
        unsafe {
            self.target.SetTransform(&Matrix3x2 {
                M11: part,
                M12: 0.0,
                M21: 0.0,
                M22: part,
                M31: rect.left,
                M32: rect.top,
            });
            self.brush.SetColor(&tint(colour));
            for stroke in icon.strokes {
                match stroke {
                    Stroke::RoundRect(x, y, width, height, radius) => {
                        self.target.DrawRoundedRectangle(
                            &D2D1_ROUNDED_RECT {
                                rect: Rect::at(*x, *y, *width, *height).d2d(),
                                radiusX: *radius,
                                radiusY: *radius,
                            },
                            &self.brush,
                            icon.thickness,
                            &self.style,
                        )
                    }
                    Stroke::SvgPath(said) => {
                        if let Some(path) = self.path_of(said) {
                            self.target.DrawGeometry(
                                &path,
                                &self.brush,
                                icon.thickness,
                                &self.style,
                            );
                        }
                    }
                }
            }
            self.target.SetTransform(&Matrix3x2 {
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
    fn path_of(&self, said: &'static str) -> Option<ID2D1PathGeometry> {
        if let Some((_, found)) = self
            .paths
            .borrow()
            .iter()
            .find(|(other, _)| std::ptr::eq(*other, said))
        {
            return found.clone();
        }
        let made = self.read_path(said);
        if made.is_none() {
            // Dit et non tu. Une icône est faite de plusieurs traits :
            // celui qui ne se lit pas disparaît, les autres restent, et
            // ce qui s'affiche est une icône méconnaissable dont rien ne
            // dit qu'elle est incomplète. C'est arrivé une fois, à
            // l'oeil barré du menu, dont le contour est la seule courbe
            // de Bézier du produit.
            note(&format!("dessin : chemin non lu, « {said} »"));
        }
        self.paths.borrow_mut().push((said, made.clone()));
        made
    }

    /// Lit un « d » de chemin SVG et en fait une forme.
    ///
    /// Ce qui est compris est ce dont les icônes de ce produit se
    /// servent, et rien de plus : aller à, tracer jusqu'à, tracer à
    /// l'horizontale, à la verticale, une courbe, un arc, et refermer.
    /// Une lettre inconnue arrête la lecture plutôt que d'être sautée :
    /// une icône à moitié dessinée ressemble à un défaut, une icône
    /// absente à un oubli, et le second se cherche. C'est `path` qui le
    /// dit à voix haute.
    fn read_path(&self, said: &str) -> Option<ID2D1PathGeometry> {
        // SAFETY: une forme et son embouchure à nous, refermées avant de
        // sortir.
        unsafe {
            let shape = self.factory.CreatePathGeometry().ok()?;
            let sink = shape.Open().ok()?;
            let mut words = Tokens::over(said);
            let (mut at, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
            let mut figure_open = false;
            let mut letter = ' ';
            while let Some(next) = words.letter_or_number() {
                if let Some(this_one) = next {
                    letter = this_one;
                }
                let relative = letter.is_lowercase();
                let mut number = || words.number();
                match letter.to_ascii_uppercase() {
                    'M' => {
                        let (x, y) = (number()?, number()?);
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        if figure_open {
                            sink.EndFigure(D2D1_FIGURE_END_OPEN);
                        }
                        sink.BeginFigure(point(at), D2D1_FIGURE_BEGIN_HOLLOW);
                        start = at;
                        figure_open = true;
                        letter = if relative { 'l' } else { 'L' };
                    }
                    'L' => {
                        let (x, y) = (number()?, number()?);
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        sink.AddLine(point(at));
                    }
                    'H' => {
                        let x = number()?;
                        at.0 = if relative { at.0 + x } else { x };
                        sink.AddLine(point(at));
                    }
                    'V' => {
                        let y = number()?;
                        at.1 = if relative { at.1 + y } else { y };
                        sink.AddLine(point(at));
                    }
                    'C' => {
                        // Les deux poignées se comptent depuis le point
                        // d'où la courbe part, donc avant de l'avoir
                        // quitté.
                        let (x1, y1) = (number()?, number()?);
                        let (x2, y2) = (number()?, number()?);
                        let (x, y) = (number()?, number()?);
                        let (first_control, second_control) = if relative {
                            ((at.0 + x1, at.1 + y1), (at.0 + x2, at.1 + y2))
                        } else {
                            ((x1, y1), (x2, y2))
                        };
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                            point1: point(first_control),
                            point2: point(second_control),
                            point3: point(at),
                        });
                    }
                    'A' => {
                        let (rx, ry) = (number()?, number()?);
                        let rotation = number()?;
                        let (large_arc, sweep) = (number()?, number()?);
                        let (x, y) = (number()?, number()?);
                        at = if relative {
                            (at.0 + x, at.1 + y)
                        } else {
                            (x, y)
                        };
                        sink.AddArc(&D2D1_ARC_SEGMENT {
                            point: point(at),
                            size: D2D_SIZE_F {
                                width: rx,
                                height: ry,
                            },
                            rotationAngle: rotation,
                            sweepDirection: if sweep != 0.0 {
                                D2D1_SWEEP_DIRECTION_CLOCKWISE
                            } else {
                                D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                            },
                            arcSize: if large_arc != 0.0 {
                                D2D1_ARC_SIZE_LARGE
                            } else {
                                D2D1_ARC_SIZE_SMALL
                            },
                        });
                    }
                    'Z' => {
                        if figure_open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            figure_open = false;
                        }
                        at = start;
                    }
                    _ => return None,
                }
            }
            if figure_open {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close().ok()?;
            Some(shape)
        }
    }
}

/// Ce qu'un chemin SVG dit, lettre par lettre et nombre par nombre.
///
/// Un signe moins ouvre un nombre, il ne sépare pas : c'est la règle de
/// ce langage, et c'est ce qui permet d'écrire « a9 9 0 1 1-12.8 0 » sans
/// espace avant le douze.
struct Tokens<'a> {
    rest: &'a str,
}

impl<'a> Tokens<'a> {
    fn over(said: &'a str) -> Self {
        Tokens { rest: said }
    }

    fn skip(&mut self) {
        self.rest = self.rest.trim_start_matches([' ', ',', '\t', '\n']);
    }

    /// La prochaine chose : une lettre, ou rien quand c'est un nombre qui
    /// vient, ou la fin.
    fn letter_or_number(&mut self) -> Option<Option<char>> {
        self.skip();
        let first = self.rest.chars().next()?;
        if first.is_ascii_alphabetic() {
            self.rest = &self.rest[first.len_utf8()..];
            return Some(Some(first));
        }
        Some(None)
    }

    fn number(&mut self) -> Option<f32> {
        self.skip();
        let mut end = 0;
        for (at, character) in self.rest.char_indices() {
            let open = at == 0 && (character == '-' || character == '+');
            if character.is_ascii_digit() || character == '.' || open {
                end = at + character.len_utf8();
            } else {
                break;
            }
        }
        if end == 0 {
            return None;
        }
        let (read, rest) = self.rest.split_at(end);
        self.rest = rest;
        read.parse().ok()
    }
}

/// Un point, dans les nombres que le dessinateur attend.
fn point(at: (f32, f32)) -> Vector2 {
    Vector2 { X: at.0, Y: at.1 }
}

/// Un mot dans les caractères que Windows compte, qui ne sont pas ceux
/// de Rust.
fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
