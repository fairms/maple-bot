use core::slice::SlicePattern;
use std::{
    collections::HashMap,
    env,
    fmt::Debug,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow};
use dyn_clone::DynClone;
use log::{debug, info};
#[cfg(test)]
use mockall::mock;
use opencv::{
    boxed_ref::BoxedRef,
    core::{
        CMP_EQ, CMP_GT, CV_8U, CV_32FC1, CV_32FC3, CV_32S, Mat, MatExprTraitConst, MatTrait,
        MatTraitConst, MatTraitConstManual, ModifyInplace, Point, Point2f, Range, Rect, Scalar,
        Size, ToInputArray, Vec4b, Vector, add, add_weighted_def, bitwise_and_def, compare,
        divide2_def, find_non_zero, min_max_loc, no_array, subtract_def, transpose_nd,
    },
    dnn::{
        ModelTrait, TextRecognitionModel, TextRecognitionModelTrait,
        TextRecognitionModelTraitConst, read_net_from_onnx_buffer,
    },
    imgcodecs::{self, IMREAD_COLOR, IMREAD_GRAYSCALE},
    imgproc::{
        CC_STAT_AREA, CC_STAT_HEIGHT, CC_STAT_LEFT, CC_STAT_TOP, CC_STAT_WIDTH,
        CHAIN_APPROX_SIMPLE, COLOR_BGRA2BGR, COLOR_BGRA2GRAY, COLOR_BGRA2RGB, INTER_AREA,
        INTER_CUBIC, MORPH_RECT, RETR_EXTERNAL, THRESH_BINARY, THRESH_OTSU, TM_CCOEFF_NORMED,
        bounding_rect, connected_components_with_stats, cvt_color_def, dilate_def,
        find_contours_def, get_structuring_element_def, match_template, min_area_rect, resize,
        resize_def, threshold,
    },
    traits::OpenCVIntoExternContainer,
};
use ort::{
    session::{Session, SessionInputValue, SessionOutputs},
    value::Tensor,
};
use platforms::windows::KeyKind;

use crate::{buff::BuffKind, mat::OwnedMat};

pub trait Detector: 'static + Send + DynClone + Debug {
    fn mat(&self) -> &OwnedMat;

    /// Detects a list of mobs.
    ///
    /// Returns a list of mobs coordinate relative to minimap coordinate.
    fn detect_mobs(&self, minimap: Rect, bound: Rect, player: Point) -> Result<Vec<Point>>;

    /// Detects whether to press ESC for unstucking.
    fn detect_esc_settings(&self) -> bool;

    /// Detects whether there is an elite boss bar.
    fn detect_elite_boss_bar(&self) -> bool;

    /// Detects the minimap.
    ///
    /// The `border_threshold` determines the "whiteness" (grayscale value from 0..255) of
    /// the minimap's white border.
    fn detect_minimap(&self, border_threshold: u8) -> Result<Rect>;

    /// Detects the portals from the given `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_minimap_portals(&self, minimap: Rect) -> Result<Vec<Rect>>;

    /// Detects the rune from the given `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect>;

    /// Detects the player in the provided `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_player(&self, minimap: Rect) -> Result<Rect>;

    /// Detects whether the player is dead.
    fn detect_player_is_dead(&self) -> bool;

    /// Detects whether the player is in cash shop.
    fn detect_player_in_cash_shop(&self) -> bool;

    /// Detects the player health bar.
    fn detect_player_health_bar(&self) -> Result<Rect>;

    /// Detects the player current and max health bars.
    fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)>;

    /// Detects the player current health and max health.
    fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)>;

    /// Detects whether the player has a buff specified by `kind`.
    fn detect_player_buff(&self, kind: BuffKind) -> bool;

    /// Detects rune arrows from the given RGBA image `Mat`.
    ///
    /// Optional `preds` can be provided to get the prediction scores, width and height ratios.
    fn detect_rune_arrows(
        &self,
        preds: Option<&mut (Vec<Vec<f32>>, f32, f32)>,
    ) -> Result<[KeyKind; 4]>;

    /// Detects 1 rune arrow from the given RGBA image `Mat` and the last three frames history.
    fn detect_rune_arrow_2(&self, last_frames: &[&OwnedMat]) -> Result<KeyKind>;

    /// Detects the Erda Shower skill from the given BGRA `Mat` image.
    fn detect_erda_shower(&self) -> Result<Rect>;
}

#[cfg(test)]
mock! {
    pub Detector {}

    impl Detector for Detector {
        fn mat(&self) -> &OwnedMat;
        fn detect_mobs(&self, minimap: Rect, bound: Rect, player: Point) -> Result<Vec<Point>>;
        fn detect_esc_settings(&self) -> bool;
        fn detect_elite_boss_bar(&self) -> bool;
        fn detect_minimap(&self, border_threshold: u8) -> Result<Rect>;
        fn detect_minimap_portals(&self, minimap: Rect) -> Result<Vec<Rect>>;
        fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect>;
        fn detect_player(&self, minimap: Rect) -> Result<Rect>;
        fn detect_player_is_dead(&self) -> bool;
        fn detect_player_in_cash_shop(&self) -> bool;
        fn detect_player_health_bar(&self) -> Result<Rect>;
        fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)>;
        fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)>;
        fn detect_player_buff(&self, kind: BuffKind) -> bool;
        fn detect_rune_arrows<'a>(
            &'a self,
            preds: Option<&'a mut (Vec<Vec<f32>>, f32, f32)>,
        ) -> Result<[KeyKind; 4]>;
        fn detect_rune_arrow_2<'a>(&'a self, last_frames: &'a [&'a OwnedMat]) -> Result<KeyKind>;
        fn detect_erda_shower(&self) -> Result<Rect>;
    }

    impl Debug for Detector {
        fn fmt<'a, 'b, 'c>(&'a self, f: &'b mut std::fmt::Formatter<'c> ) -> std::fmt::Result;
    }

    impl Clone for Detector {
        fn clone(&self) -> Self;
    }
}

type MatFn = Box<dyn FnOnce() -> Mat + Send>;

/// A detector that temporary caches the transformed `Mat`.
///
/// It is useful when there are multiple detections in a single tick that
/// rely on grayscale (e.g. buffs).
///
/// TODO: Is it really useful?
#[derive(Clone, Debug)]
pub struct CachedDetector {
    mat: Arc<OwnedMat>,
    grayscale: Arc<LazyLock<Mat, MatFn>>,
    buffs_grayscale: Arc<LazyLock<Mat, MatFn>>,
}

impl CachedDetector {
    pub fn new(mat: OwnedMat) -> CachedDetector {
        let mat = Arc::new(mat);
        let grayscale = mat.clone();
        let grayscale = Arc::new(LazyLock::<Mat, MatFn>::new(Box::new(move || {
            to_grayscale(&*grayscale, true)
        })));
        let buffs_grayscale = grayscale.clone();
        let buffs_grayscale = Arc::new(LazyLock::<Mat, MatFn>::new(Box::new(move || {
            crop_to_buffs_region(&**buffs_grayscale).clone_pointee()
        })));
        Self {
            mat,
            grayscale,
            buffs_grayscale,
        }
    }
}

impl Detector for CachedDetector {
    fn mat(&self) -> &OwnedMat {
        &self.mat
    }

    fn detect_mobs(&self, minimap: Rect, bound: Rect, player: Point) -> Result<Vec<Point>> {
        detect_mobs(&*self.mat, minimap, bound, player)
    }

    fn detect_esc_settings(&self) -> bool {
        detect_esc_settings(&**self.grayscale)
    }

    fn detect_elite_boss_bar(&self) -> bool {
        detect_elite_boss_bar(&**self.grayscale)
    }

    fn detect_minimap(&self, border_threshold: u8) -> Result<Rect> {
        detect_minimap(&*self.mat, border_threshold)
    }

    fn detect_minimap_portals(&self, minimap: Rect) -> Result<Vec<Rect>> {
        let minimap_color = to_bgr(&self.mat.roi(minimap)?);
        detect_minimap_portals(minimap_color)
    }

    fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect> {
        let minimap_grayscale = self.grayscale.roi(minimap)?;
        detect_minimap_rune(&minimap_grayscale)
    }

    fn detect_player(&self, minimap: Rect) -> Result<Rect> {
        let minimap_grayscale = self.grayscale.roi(minimap)?;
        detect_player(&minimap_grayscale)
    }

    fn detect_player_is_dead(&self) -> bool {
        detect_player_is_dead(&**self.grayscale)
    }

    fn detect_player_in_cash_shop(&self) -> bool {
        detect_player_in_cash_shop(&**self.grayscale)
    }

    fn detect_player_health_bar(&self) -> Result<Rect> {
        detect_player_health_bar(&**self.grayscale)
    }

    fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)> {
        detect_player_health_bars(&*self.mat, &**self.grayscale, health_bar)
    }

    fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)> {
        detect_player_health(&*self.mat, current_bar, max_bar)
    }

    fn detect_player_buff(&self, kind: BuffKind) -> bool {
        let mat = match kind {
            BuffKind::Rune
            | BuffKind::SayramElixir
            | BuffKind::AureliaElixir
            | BuffKind::ExpCouponX3
            | BuffKind::BonusExpCoupon => &**self.buffs_grayscale,
            BuffKind::LegionWealth
            | BuffKind::LegionLuck
            | BuffKind::WealthAcquisitionPotion
            | BuffKind::ExpAccumulationPotion
            | BuffKind::ExtremeRedPotion
            | BuffKind::ExtremeBluePotion
            | BuffKind::ExtremeGreenPotion
            | BuffKind::ExtremeGoldPotion => &to_bgr(&crop_to_buffs_region(&*self.mat)),
        };
        detect_player_buff(mat, kind)
    }

    fn detect_rune_arrows(
        &self,
        preds: Option<&mut (Vec<Vec<f32>>, f32, f32)>,
    ) -> Result<[KeyKind; 4]> {
        detect_rune_arrows(&*self.mat, preds)
    }
    fn detect_rune_arrow_2(&self, last_frames: &[&OwnedMat]) -> Result<KeyKind> {
        detect_rune_arrows_2(self.mat(), last_frames)
    }

    fn detect_erda_shower(&self) -> Result<Rect> {
        detect_erda_shower(&**self.grayscale)
    }
}

fn crop_to_buffs_region(mat: &impl MatTraitConst) -> BoxedRef<Mat> {
    let size = mat.size().unwrap();
    // crop to top right of the image for buffs region
    let crop_x = size.width / 3;
    let crop_y = size.height / 4;
    let crop_bbox = Rect::new(size.width - crop_x, 0, crop_x, crop_y);
    mat.roi(crop_bbox).unwrap()
}

fn detect_mobs(
    mat: &impl MatTraitConst,
    minimap: Rect,
    bound: Rect,
    player: Point,
) -> Result<Vec<Point>> {
    static MOB_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("MOB_MODEL"))))
            .expect("unable to build mob detection session")
    });

    /// This function approximates the delta (dx, dy) that the player needs to move
    /// in relative to the minimap coordinate in order to reach the mob. And returns
    /// the exact mob coordinate on the minimap.
    ///
    /// Note: It is not that accurate but that is that and this is this
    #[inline]
    fn to_minimap_coordinate(
        bbox: Rect,
        minimap: Rect,
        bound: Rect,
        player: Point,
        size: Size,
    ) -> Option<Point> {
        // this is the linear transformation from screen coordinate
        // to minimap coordinate that I cooked up using alchemy
        // Is it correct? I don't know, tried others but only this sort of work
        // [ A1 A2 ]
        // [ B1 B2 ]
        const A1: f32 = 0.065_789_476;
        const A2: f32 = 0.120_621_44;
        const A: Point2f = Point2f::new(A1, A2);
        const B1: f32 = 0.0;
        const B2: f32 = 0.072_635_14;
        const B: Point2f = Point2f::new(B1, B2);

        // the main idea is to calculate the offset of the detected mob
        // from the middle of screen and use that distance as dx to move the player
        // for dy, it is calculated as offset from the bottom of the screen
        // minus some number
        // point_x is relative to middle of the screen
        let point_x = (bbox.x + bbox.width / 2) as f32;
        let point_x = size.width as f32 / 2.0 - point_x;
        let is_left = point_x > 0.0;
        let point_x = Point2f::new(point_x.abs(), 0.0);

        // point_y is relative to top of the screen
        let point_y = (bbox.y + bbox.height) as f32;
        let point_y = Point2f::new(0.0, size.height as f32 - point_y);

        // transform to minimap coordinate
        // 20.0 is a based random number
        let point = Point2f::new(point_x.dot(A), point_y.dot(B) - 20.0)
            .to::<i32>()
            .unwrap();
        let point = if is_left {
            Point::new(player.x - point.x, player.y + point.y)
        } else {
            Point::new(player.x + point.x, player.y + point.y)
        };
        let point = Point::new(point.x, minimap.height - point.y);
        if point.x < 0
            || point.y < 0
            || point.x < bound.x
            || point.x > bound.x + bound.width
            || point.y < bound.y
            || point.y > bound.y + bound.height
        {
            None
        } else {
            debug!(target: "mob", "found mob {point:?} in bound {bound:?}");
            Some(point)
        }
    }

    let size = mat.size().unwrap();
    let (mat_in, w_ratio, h_ratio) = preprocess_for_yolo(mat);
    let result = MOB_MODEL.run([norm_rgb_to_input_value(&mat_in)]).unwrap();
    let result = from_output_value(&result);
    // SAFETY: 0..result.rows() is within Mat bounds
    let points = (0..result.rows())
        .map(|i| unsafe { result.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|pred| pred[4] >= 0.5)
        .map(|pred| remap_from_yolo(pred, size, w_ratio, h_ratio))
        .filter_map(|bbox| to_minimap_coordinate(bbox, minimap, bound, player, size))
        .collect::<Vec<_>>();
    // let bboxes = (0..result.rows())
    //     .map(|i| unsafe { result.at_row_unchecked::<f32>(i).unwrap() })
    //     .filter_map(|pred| if pred[4] > 0.5 { Some(pred) } else { None })
    //     .map(|pred| remap_from_yolo(pred, size, w_ratio, h_ratio))
    //     .collect::<Vec<_>>();
    // let points = bboxes
    //     .iter()
    //     .copied()
    //     .filter_map(|bbox| to_minimap_coordinate(bbox, minimap, bound, player, size))
    //     .collect::<Vec<_>>();
    // #[cfg(debug_assertions)]
    // if !bboxes.is_empty() {
    //     debug_mat(
    //         "Test",
    //         mat,
    //         1,
    //         &points
    //             .clone()
    //             .into_iter()
    //             .map(|pt| minimap.tl() + Point::new(pt.x, pt.y))
    //             .map(|pt| Rect::from_points(pt - Point::new(5, 5), pt))
    //             .chain(bboxes)
    //             .collect::<Vec<_>>(),
    //         &vec![""; points.len() * 2],
    //     );
    // }
    Ok(points)
}

fn detect_esc_settings(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static ESC_SETTINGS: LazyLock<[Mat; 7]> = LazyLock::new(|| {
        [
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_SETTING_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
            imgcodecs::imdecode(include_bytes!(env!("ESC_MENU_TEMPLATE")), IMREAD_GRAYSCALE)
                .unwrap(),
            imgcodecs::imdecode(include_bytes!(env!("ESC_EVENT_TEMPLATE")), IMREAD_GRAYSCALE)
                .unwrap(),
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_COMMUNITY_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_CHARACTER_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
            imgcodecs::imdecode(include_bytes!(env!("ESC_OK_TEMPLATE")), IMREAD_GRAYSCALE).unwrap(),
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_CANCEL_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
        ]
    });

    for template in &*ESC_SETTINGS {
        if detect_template(mat, template, Point::default(), 0.85).is_ok() {
            return true;
        }
    }
    false
}

fn detect_elite_boss_bar(mat: &impl MatTraitConst) -> bool {
    /// TODO: Support default ratio
    static ELITE_BOSS_BAR_1: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ELITE_BOSS_BAR_1_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static ELITE_BOSS_BAR_2: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ELITE_BOSS_BAR_2_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let size = mat.size().unwrap();
    // crop to top part of the image for boss bar
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(0, 0, size.width, crop_y);
    let boss_bar = mat.roi(crop_bbox).unwrap();
    let template_1 = &*ELITE_BOSS_BAR_1;
    let template_2 = &*ELITE_BOSS_BAR_2;
    detect_template(&boss_bar, template_1, Point::default(), 0.9).is_ok()
        || detect_template(&boss_bar, template_2, Point::default(), 0.9).is_ok()
}

fn detect_minimap(mat: &impl MatTraitConst, border_threshold: u8) -> Result<Rect> {
    static MINIMAP_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("MINIMAP_MODEL"))))
            .expect("unable to build minimap detection session")
    });
    // expands out a few pixels to include the whole white border for thresholding
    // after yolo detection
    fn expand_bbox(bbox: &Rect) -> Rect {
        let count = (bbox.width.max(bbox.height) as f32 * 0.008).ceil() as i32;
        debug!(target: "minimap", "expand border by {count}");
        let x = (bbox.x - count).max(0);
        let y = (bbox.y - count).max(0);
        let x_size = (bbox.x - x) * 2;
        let y_size = (bbox.y - y) * 2;
        Rect::new(x, y, bbox.width + x_size, bbox.height + y_size)
    }

    let size = mat.size().unwrap();
    let (preprocessed, w_ratio, h_ratio) = preprocess_for_yolo(mat);
    let result = MINIMAP_MODEL
        .run([norm_rgb_to_input_value(&preprocessed)])
        .unwrap();
    let result = from_output_value(&result);
    let pred = (0..result.rows())
        // SAFETY: 0..result.rows() is within Mat bounds
        .map(|i| unsafe { result.at_row_unchecked::<f32>(i).unwrap() })
        .max_by(|&a, &b| {
            // a and b have shapes [bbox(4) + class(1)]
            a[4].total_cmp(&b[4])
        });
    let bbox = pred.and_then(|pred| {
        debug!(target: "minimap", "yolo detection: {pred:?}");
        if pred[4] < 0.5 {
            None
        } else {
            Some(remap_from_yolo(pred, size, w_ratio, h_ratio))
        }
    });
    let minimap = bbox.map(|bbox| {
        let bbox = expand_bbox(&bbox);
        let mut minimap = to_grayscale(&mat.roi(bbox).unwrap(), true);
        unsafe {
            // SAFETY: threshold can be called in place.
            minimap.modify_inplace(|mat, mat_mut| {
                threshold(mat, mat_mut, 0.0, 255.0, THRESH_OTSU).unwrap()
            });
        }
        minimap
    });
    // get only the outer contours
    let contours = minimap.map(|mat| {
        let mut vec = Vector::<Vector<Point>>::new();
        find_contours_def(&mat, &mut vec, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE).unwrap();
        vec
    });
    // pick the contour with maximum area
    let contour = contours.and_then(|vec| {
        debug!(target: "minimap", "contours detection: {vec:?}");
        vec.into_iter()
            .map(|contour| bounding_rect(&contour).unwrap())
            .max_by(|a, b| a.area().cmp(&b.area()))
    });
    let contour = contour.and_then(|contour| {
        let bbox = expand_bbox(&bbox.unwrap());
        let contour = Rect::from_points(contour.tl() + bbox.tl(), contour.br() + bbox.tl());
        debug!(
            target: "minimap",
            "yolo bbox and contour bbox areas: {:?} {:?}",
            bbox.area(),
            contour.area()
        );
        // the detected contour should be contained inside the detected yolo minimap when expanded
        // <some value that i will probably change again the future> is a
        // fixed value for ensuring the contour is tight to the minimap white border
        if (bbox & contour) == contour && (bbox.area() - contour.area()) >= 1100 {
            Some(contour)
        } else {
            None
        }
    });
    // crop the white border
    let crop = contour.and_then(|bound| {
        // Offset in by 10% to avoid the round border
        // and use top border as basis
        let range = (bound.width as f32 * 0.1) as i32;
        let start = bound.x + range;
        let end = bound.x + bound.width - range + 1;
        // Count for the number of pixels larger than threshold
        // starting from bound's y. Use the maximum count as the number of pixels to crop.
        let mut counts = HashMap::<i32, i32>::new();
        for col in start..end {
            let mut count = 0;
            for row in bound.y..(bound.y + bound.height) {
                if mat
                    .at_2d::<Vec4b>(row, col)
                    .unwrap()
                    .iter()
                    .all(|v| *v >= border_threshold)
                {
                    count += 1;
                } else {
                    break;
                }
            }
            counts.entry(count).and_modify(|c| *c += 1).or_insert(1);
        }
        debug!(target: "minimap", "border pixel count {:?}", counts);
        counts.into_iter().max_by(|a, b| a.1.cmp(&b.1)).map(|e| e.0)
    });
    crop.map(|count| {
        let contour = contour.unwrap();
        Rect::new(
            contour.x + count,
            contour.y + count,
            contour.width - count * 2,
            contour.height - count * 2,
        )
    })
    .ok_or(anyhow!("minimap not found"))
}

fn detect_minimap_portals<T: MatTraitConst + ToInputArray>(minimap: T) -> Result<Vec<Rect>> {
    /// TODO: Support default ratio
    static PORTAL: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("PORTAL_TEMPLATE")), IMREAD_COLOR).unwrap()
    });

    let template = &*PORTAL;
    let mut result = Mat::default();
    let mut points = Vector::<Point>::new();
    match_template(
        &minimap,
        template,
        &mut result,
        TM_CCOEFF_NORMED,
        &no_array(),
    )
    .unwrap();
    // SAFETY: threshold can be called inplace
    unsafe {
        result.modify_inplace(|mat, mat_mut| {
            threshold(mat, mat_mut, 0.8, 1.0, THRESH_BINARY).unwrap();
        });
    }
    find_non_zero(&result, &mut points).unwrap();
    let portals = points
        .into_iter()
        .map(|point| {
            let size = 5;
            let x = (point.x - size).max(0);
            let xd = point.x - x;
            let y = (point.y - size).max(0);
            let yd = point.y - y;
            let width = template.cols() + xd * 2 + (size - xd);
            let height = template.rows() + yd * 2 + (size - yd);
            Rect::new(x, y, width, height)
        })
        .collect::<Vec<_>>();
    Ok(portals)
}

fn detect_minimap_rune(minimap: &impl ToInputArray) -> Result<Rect> {
    /// TODO: Support default ratio
    static RUNE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(minimap, &*RUNE, Point::default(), 0.6)
}

fn detect_player(mat: &impl ToInputArray) -> Result<Rect> {
    const PLAYER_IDEAL_RATIO_THRESHOLD: f64 = 0.75;
    const PLAYER_DEFAULT_RATIO_THRESHOLD: f64 = 0.6;
    static PLAYER_IDEAL_RATIO: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_IDEAL_RATIO_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static PLAYER_DEFAULT_RATIO: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_DEFAULT_RATIO_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static WAS_IDEAL_RATIO: AtomicBool = AtomicBool::new(false);

    let was_ideal_ratio = WAS_IDEAL_RATIO.load(Ordering::Acquire);
    let template = if was_ideal_ratio {
        &*PLAYER_IDEAL_RATIO
    } else {
        &*PLAYER_DEFAULT_RATIO
    };
    let threshold = if was_ideal_ratio {
        PLAYER_IDEAL_RATIO_THRESHOLD
    } else {
        PLAYER_DEFAULT_RATIO_THRESHOLD
    };
    let result = detect_template(mat, template, Point::default(), threshold);
    if result.is_err() {
        WAS_IDEAL_RATIO.store(!was_ideal_ratio, Ordering::Release);
    }
    result
}

fn detect_player_is_dead(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("TOMB_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(mat, &*TEMPLATE, Point::default(), 0.8).is_ok()
}

fn detect_player_in_cash_shop(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static CASH_SHOP: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("CASH_SHOP_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(mat, &*CASH_SHOP, Point::default(), 0.7).is_ok()
}

fn detect_player_health_bar(mat: &impl ToInputArray) -> Result<Rect> {
    /// TODO: Support default ratio
    static HP_START: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HP_START_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });
    static HP_END: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HP_END_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    let hp_start = detect_template(mat, &*HP_START, Point::default(), 0.8)?;
    let hp_start_to_edge_x = hp_start.x + hp_start.width;
    let hp_end = detect_template(mat, &*HP_END, Point::default(), 0.8)?;
    Ok(Rect::new(
        hp_start_to_edge_x,
        hp_start.y,
        hp_end.x - hp_start_to_edge_x,
        hp_start.height,
    ))
}

fn detect_player_health_bars(
    mat: &impl MatTraitConst,
    grayscale: &impl MatTraitConst,
    hp_bar: Rect,
) -> Result<(Rect, Rect)> {
    /// TODO: Support default ratio
    static HP_SEPARATOR_1: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HP_SEPARATOR_1_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static HP_SEPARATOR_2: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HP_SEPARATOR_2_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static HP_SHIELD: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HP_SHIELD_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });
    static HP_SEPARATOR_TYPE_1: AtomicBool = AtomicBool::new(true);

    let hp_separator_type_1 = HP_SEPARATOR_TYPE_1.load(Ordering::Relaxed);
    let hp_separator_template = if hp_separator_type_1 {
        &*HP_SEPARATOR_1
    } else {
        &*HP_SEPARATOR_2
    };
    let hp_separator = detect_template(
        &grayscale.roi(hp_bar).unwrap(),
        hp_separator_template,
        hp_bar.tl(),
        0.7,
    )
    .inspect_err(|_| {
        HP_SEPARATOR_TYPE_1.store(!hp_separator_type_1, Ordering::Release);
    })?;
    let hp_shield = detect_template(
        &grayscale.roi(hp_bar).unwrap(),
        &*HP_SHIELD,
        hp_bar.tl(),
        0.8,
    )
    .ok();
    let left = mat
        .roi(Rect::new(
            hp_bar.x,
            hp_bar.y,
            hp_separator.x - hp_bar.x,
            hp_bar.height,
        ))
        .unwrap();
    let (left_in, left_w_ratio, left_h_ratio) = preprocess_for_text_bboxes(&left);
    let left_bbox = extract_text_bboxes(&left_in, left_w_ratio, left_h_ratio, hp_bar.x, hp_bar.y)
        .into_iter()
        .min_by_key(|bbox| ((bbox.x + bbox.width) - hp_separator.x).abs())
        .ok_or(anyhow!("failed to detect current health bar"))?;
    let left_bbox_x = hp_shield
        .map(|bbox| bbox.x + bbox.width)
        .unwrap_or(left_bbox.x); // When there is shield, skips past it
    let left_bbox = Rect::new(
        left_bbox_x,
        left_bbox.y - 1, // Add some space so the bound is not too tight
        hp_separator.x - left_bbox_x + 1, // Help thin character like '1' detectable
        left_bbox.height + 2,
    );
    let right = mat
        .roi(Rect::new(
            hp_separator.x + hp_separator.width,
            hp_bar.y,
            (hp_bar.x + hp_bar.width) - (hp_separator.x + hp_separator.width),
            hp_bar.height,
        ))
        .unwrap();
    let (right_in, right_w_ratio, right_h_ratio) = preprocess_for_text_bboxes(&right);
    let right_bbox = extract_text_bboxes(
        &right_in,
        right_w_ratio,
        right_h_ratio,
        hp_separator.x + hp_separator.width,
        hp_bar.y,
    )
    .into_iter()
    .reduce(|acc, cur| acc | cur)
    .ok_or(anyhow!("failed to detect max health bar"))?;
    Ok((left_bbox, right_bbox))
}

fn detect_player_health(
    mat: &impl MatTraitConst,
    current_bar: Rect,
    max_bar: Rect,
) -> Result<(u32, u32)> {
    let current_health = extract_texts(mat, &[current_bar]);
    let current_health = current_health
        .first()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(anyhow!("cannot detect current health"))?;
    let max_health = extract_texts(mat, &[max_bar]);
    let max_health = max_health
        .first()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(anyhow!("cannot detect max health"))?;
    Ok((current_health.min(max_health), max_health))
}

fn detect_player_buff<T: MatTraitConst + ToInputArray>(mat: &T, kind: BuffKind) -> bool {
    /// TODO: Support default ratio
    static RUNE_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_BUFF_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });
    static SAYRAM_ELIXIR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("SAYRAM_ELIXIR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static AURELIA_ELIXIR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("AURELIA_ELIXIR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EXP_COUPON_X3_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_COUPON_X3_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static BONUS_EXP_COUPON_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("BONUS_EXP_COUPON_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static LEGION_WEALTH_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_WEALTH_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static LEGION_LUCK_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_LUCK_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static WEALTH_EXP_POTION_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("WEALTH_EXP_POTION_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static WEALTH_ACQUISITION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("WEALTH_ACQUISITION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXP_ACCUMULATION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_ACCUMULATION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_RED_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_RED_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_BLUE_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_BLUE_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_GREEN_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_GREEN_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_GOLD_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_GOLD_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    let threshold = match kind {
        BuffKind::AureliaElixir => 0.8,
        BuffKind::LegionWealth => 0.76,
        BuffKind::Rune
        | BuffKind::SayramElixir
        | BuffKind::ExpCouponX3
        | BuffKind::BonusExpCoupon
        | BuffKind::LegionLuck
        | BuffKind::ExtremeRedPotion
        | BuffKind::ExtremeBluePotion
        | BuffKind::WealthAcquisitionPotion
        | BuffKind::ExpAccumulationPotion
        | BuffKind::ExtremeGreenPotion
        | BuffKind::ExtremeGoldPotion => 0.75,
    };
    let template = match kind {
        BuffKind::Rune => &*RUNE_BUFF,
        BuffKind::SayramElixir => &*SAYRAM_ELIXIR_BUFF,
        BuffKind::AureliaElixir => &*AURELIA_ELIXIR_BUFF,
        BuffKind::ExpCouponX3 => &*EXP_COUPON_X3_BUFF,
        BuffKind::BonusExpCoupon => &*BONUS_EXP_COUPON_BUFF,
        BuffKind::LegionWealth => &*LEGION_WEALTH_BUFF,
        BuffKind::LegionLuck => &*LEGION_LUCK_BUFF,
        BuffKind::WealthAcquisitionPotion => &*WEALTH_ACQUISITION_POTION_BUFF,
        BuffKind::ExpAccumulationPotion => &*EXP_ACCUMULATION_POTION_BUFF,
        BuffKind::ExtremeRedPotion => &*EXTREME_RED_POTION_BUFF,
        BuffKind::ExtremeBluePotion => &*EXTREME_BLUE_POTION_BUFF,
        BuffKind::ExtremeGreenPotion => &*EXTREME_GREEN_POTION_BUFF,
        BuffKind::ExtremeGoldPotion => &*EXTREME_GOLD_POTION_BUFF,
    };

    if matches!(
        kind,
        BuffKind::WealthAcquisitionPotion | BuffKind::ExpAccumulationPotion
    ) {
        // Because the two potions are really similar, detecting one may mis-detect for the other.
        // Can't really think of a better way to do this.... But this seems working just fine.
        // Also tested with the who-use-this? Invicibility Potion and Resistance Potion. Those two
        // doesn't match at all so this should be fine.
        let matches = detect_template_multiple(
            mat,
            template,
            &*WEALTH_EXP_POTION_MASK,
            Point::default(),
            2,
            threshold,
        )
        .into_iter()
        .filter_map(|result| result.ok())
        .collect::<Vec<_>>();
        if matches.is_empty() {
            return false;
        }
        // Likely both potions are active
        if matches.len() == 2 {
            return true;
        }
        let template_other = if matches!(kind, BuffKind::WealthAcquisitionPotion) {
            &*EXP_ACCUMULATION_POTION_BUFF
        } else {
            &*WEALTH_ACQUISITION_POTION_BUFF
        };
        let match_current = matches.into_iter().next().unwrap();
        let match_other = detect_template_single(
            mat,
            template_other,
            &*WEALTH_EXP_POTION_MASK,
            Point::default(),
            threshold,
        );
        if match_other.is_err() || match_other.unwrap().1 < match_current.1 {
            return true;
        }
        false
    } else {
        detect_template(mat, template, Point::default(), threshold).is_ok()
    }
}

fn detect_rune_arrows(
    mat: &impl MatTraitConst,
    preds_out: Option<&mut (Vec<Vec<f32>>, f32, f32)>,
) -> Result<[KeyKind; 4]> {
    static RUNE_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("RUNE_MODEL"))))
            .expect("unable to build rune detection session")
    });

    fn map_arrow(pred: &[f32]) -> KeyKind {
        match pred[5] as i32 {
            0 => KeyKind::Up,
            1 => KeyKind::Down,
            2 => KeyKind::Left,
            3 => KeyKind::Right,
            _ => unreachable!(),
        }
    }

    let (mat_in, w_ratio, h_ratio) = preprocess_for_yolo(mat);
    let result = RUNE_MODEL.run([norm_rgb_to_input_value(&mat_in)]).unwrap();
    let mat_out = from_output_value(&result);
    let mut preds = (0..mat_out.rows())
        // SAFETY: 0..outputs.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|&pred| {
            // pred has shapes [bbox(4) + conf + class]
            pred[4] >= 0.8
        })
        .collect::<Vec<_>>();
    if preds.len() != 4 {
        info!(target: "player", "failed to detect rune arrows {preds:?}");
        return Err(anyhow!("failed to detect rune arrows"));
    }
    // sort by x for arrow order
    preds.sort_by(|&a, &b| a[0].total_cmp(&b[0]));

    if let Some(preds_out) = preds_out {
        preds_out.0.extend(preds.iter().map(|pred| pred.to_vec()));
        preds_out.1 = w_ratio;
        preds_out.2 = h_ratio;
    }

    let first = map_arrow(preds[0]);
    let second = map_arrow(preds[1]);
    let third = map_arrow(preds[2]);
    let fourth = map_arrow(preds[3]);
    info!(
        target: "player",
        "solving rune result {first:?} ({}), {second:?} ({}), {third:?} ({}), {fourth:?} ({})",
        preds[0][4],
        preds[1][4],
        preds[2][4],
        preds[3][4]
    );
    Ok([first, second, third, fourth])
}

fn detect_rune_arrows_2(mat: &OwnedMat, last_frames: &[&OwnedMat]) -> Result<KeyKind> {
    static RUNE_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("RUNE_2_MODEL"))))
            .expect("unable to build rune detection session")
    });
    const SIZE: i32 = 96;

    let mut first = mat.try_clone().unwrap();
    let mut second = last_frames[0].try_clone().unwrap();
    let mut third = last_frames[1].try_clone().unwrap();
    let mut fourth = last_frames[2].try_clone().unwrap();

    unsafe {
        first.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize_def(mat, mat_mut, Size::new(SIZE, SIZE)).unwrap();
            mat.convert_to_def(mat_mut, CV_32FC3).unwrap();
        });
        second.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize_def(mat, mat_mut, Size::new(SIZE, SIZE)).unwrap();
            mat.convert_to_def(mat_mut, CV_32FC3).unwrap();
        });
        third.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize_def(mat, mat_mut, Size::new(SIZE, SIZE)).unwrap();
            mat.convert_to_def(mat_mut, CV_32FC3).unwrap();
        });
        fourth.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize_def(mat, mat_mut, Size::new(SIZE, SIZE)).unwrap();
            mat.convert_to_def(mat_mut, CV_32FC3).unwrap();
        });
    }

    let first = first.reshape_nd(1, &[3, SIZE, SIZE]).unwrap();
    let second = second.reshape_nd(1, &[3, SIZE, SIZE]).unwrap();
    let third = third.reshape_nd(1, &[3, SIZE, SIZE]).unwrap();
    let fourth = fourth.reshape_nd(1, &[3, SIZE, SIZE]).unwrap();

    let tensor = Tensor::from_array((
        [1, 12, 96, 96],
        [
            first.data_typed::<f32>().unwrap(),
            second.data_typed::<f32>().unwrap(),
            third.data_typed::<f32>().unwrap(),
            fourth.data_typed::<f32>().unwrap(),
        ]
        .concat(),
    ))
    .unwrap();
    let input = SessionInputValue::Owned(tensor.into_dyn());
    let output = RUNE_MODEL.run([input]).unwrap();
    let output = output["output_0"].try_extract_raw_tensor::<i64>();
    println!("{:?}", output);

    // hconcat2(src1, src2, dst)
    Err(anyhow!("asasda"))
}

fn detect_erda_shower(mat: &impl MatTraitConst) -> Result<Rect> {
    /// TODO: Support default ratio
    static ERDA_SHOWER: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ERDA_SHOWER_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let size = mat.size().unwrap();
    // crop to bottom right of the image for skill bar
    let crop_x = size.width / 2;
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(size.width - crop_x, size.height - crop_y, crop_x, crop_y);
    let skill_bar = mat.roi(crop_bbox).unwrap();
    detect_template(&skill_bar, &*ERDA_SHOWER, crop_bbox.tl(), 0.96)
}

/// Detects a single match from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    offset: Point,
    threshold: f64,
) -> Result<Rect> {
    detect_template_single(mat, template, no_array(), offset, threshold).map(|(bbox, _)| bbox)
}

/// Detects a single match with `mask` from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template_single<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    mask: impl ToInputArray,
    offset: Point,
    threshold: f64,
) -> Result<(Rect, f64)> {
    detect_template_multiple(mat, template, mask, offset, 1, threshold)
        .into_iter()
        .next()
        .unwrap()
}

/// Detects multiple matches from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template_multiple<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    mask: impl ToInputArray,
    offset: Point,
    max_matches: usize,
    threshold: f64,
) -> Vec<Result<(Rect, f64)>> {
    #[inline]
    fn match_one(
        result: &Mat,
        offset: Point,
        template_size: Size,
        threshold: f64,
    ) -> Result<(Rect, f64)> {
        let mut score = 0f64;
        let mut loc = Point::default();
        min_max_loc(
            &result,
            None,
            Some(&mut score),
            None,
            Some(&mut loc),
            &no_array(),
        )
        .unwrap();
        if score < threshold {
            return Err(anyhow!("template not found").context(score));
        }
        let tl = loc + offset;
        let br = tl + Point::from_size(template_size);
        let rect = Rect::from_points(tl, br);
        Ok((rect, score))
    }

    let mut result = Mat::default();
    match_template(mat, template, &mut result, TM_CCOEFF_NORMED, &mask).unwrap();

    let template_size = template.size().unwrap();
    let max_matches = max_matches.max(1);
    if max_matches == 1 {
        return vec![match_one(&result, offset, template_size, threshold)];
    }

    let mut filter = Vec::new();
    let zeros = Mat::zeros(template_size.height, template_size.width, CV_32FC1)
        .unwrap()
        .to_mat()
        .unwrap();
    for _ in 0..max_matches {
        let match_result = match_one(&result, offset, template_size, threshold);
        if match_result.is_err() {
            filter.push(match_result);
            continue;
        }
        let (rect, score) = match_result.unwrap();
        let mut roi = result.roi_mut(rect).unwrap();
        zeros.copy_to(&mut roi).unwrap();
        filter.push(Ok((rect, score)));
    }
    filter
}

/// Extracts texts from the non-preprocessed `Mat` and detected text bounding boxes.
fn extract_texts(mat: &impl MatTraitConst, bboxes: &[Rect]) -> Vec<String> {
    static TEXT_RECOGNITION_MODEL: LazyLock<Mutex<TextRecognitionModel>> = LazyLock::new(|| {
        let model = read_net_from_onnx_buffer(&Vector::from_slice(include_bytes!(env!(
            "TEXT_RECOGNITION_MODEL"
        ))))
        .unwrap();
        Mutex::new(
            TextRecognitionModel::new(&model)
                .and_then(|mut m| {
                    m.set_input_params(
                        1.0 / 127.5,
                        Size::new(100, 32),
                        Scalar::new(127.5, 127.5, 127.5, 0.0),
                        false,
                        false,
                    )?;
                    m.set_decode_type("CTC-greedy")?.set_vocabulary(
                        &include_str!(env!("TEXT_RECOGNITION_ALPHABET"))
                            .lines()
                            .collect::<Vector<String>>(),
                    )
                })
                .expect("unable to build text recognition model"),
        )
    });

    let recognizier = TEXT_RECOGNITION_MODEL.lock().unwrap();
    bboxes
        .iter()
        .copied()
        .filter_map(|word| {
            let mut mat = mat.roi(word).unwrap().clone_pointee();
            unsafe {
                mat.modify_inplace(|mat, mat_mut| {
                    cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
                });
            }
            recognizier.recognize(&mat).ok()
        })
        .collect()
}

/// Extracts text bounding boxes from the preprocessed [`Mat`].
///
/// This function is adapted from
/// https://github.com/clovaai/CRAFT-pytorch/blob/master/craft_utils.py#L19 with minor changes
fn extract_text_bboxes(
    mat_in: &impl MatTraitConst,
    w_ratio: f32,
    h_ratio: f32,
    x_offset: i32,
    y_offset: i32,
) -> Vec<Rect> {
    const TEXT_SCORE_THRESHOLD: f64 = 0.7;
    const LINK_SCORE_THRESHOLD: f64 = 0.4;
    static TEXT_DETECTION_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("TEXT_DETECTION_MODEL"))))
            .expect("unable to build minimap name detection session")
    });

    let result = TEXT_DETECTION_MODEL
        .run([norm_rgb_to_input_value(mat_in)])
        .unwrap();
    let mat = from_output_value(&result);
    let text_score = mat
        .ranges(&Vector::from_iter([
            Range::all().unwrap(),
            Range::all().unwrap(),
            Range::new(0, 1).unwrap(),
        ]))
        .unwrap()
        .clone_pointee();
    // remove last channel (not sure what other way to do it without clone_pointee first)
    let text_score = text_score
        .reshape_nd(1, &text_score.mat_size().as_slice()[..2])
        .unwrap();

    let mut text_low_score = Mat::default();
    threshold(
        &text_score,
        &mut text_low_score,
        LINK_SCORE_THRESHOLD,
        1.0,
        THRESH_BINARY,
    )
    .unwrap();

    let mut link_score = mat
        .ranges(&Vector::from_iter([
            Range::all().unwrap(),
            Range::all().unwrap(),
            Range::new(1, 2).unwrap(),
        ]))
        .unwrap()
        .clone_pointee();
    // remove last channel (not sure what other way to do it without clone_pointee first)
    let mut link_score = link_score
        .reshape_nd_mut(1, &link_score.mat_size().as_slice()[..2])
        .unwrap();
    // SAFETY: can be modified in place
    unsafe {
        link_score.modify_inplace(|mat, mat_mut| {
            threshold(mat, mat_mut, LINK_SCORE_THRESHOLD, 1.0, THRESH_BINARY).unwrap();
        });
    }

    let mut combined_score = Mat::default();
    let mut gt_one_mask = Mat::default();
    add(
        &text_low_score,
        &link_score,
        &mut combined_score,
        &no_array(),
        CV_8U,
    )
    .unwrap();
    compare(&combined_score, &Scalar::all(1.0), &mut gt_one_mask, CMP_GT).unwrap();
    combined_score
        .set_to(&Scalar::all(1.0), &gt_one_mask)
        .unwrap();

    let mut bboxes = Vec::<Rect>::new();
    let mut labels = Mat::default();
    let mut stats = Mat::default();
    let labels_count = connected_components_with_stats(
        &combined_score,
        &mut labels,
        &mut stats,
        &mut Mat::default(),
        4,
        CV_32S,
    )
    .unwrap();
    for i in 1..labels_count {
        let area = *stats.at_2d::<i32>(i, CC_STAT_AREA).unwrap();
        if area < 10 {
            continue;
        }
        let mut mask = Mat::default();
        let mut max_score = 0.0f64;
        compare(&labels, &Scalar::all(i as f64), &mut mask, CMP_EQ).unwrap();
        min_max_loc(&text_score, None, Some(&mut max_score), None, None, &mask).unwrap();
        if max_score < TEXT_SCORE_THRESHOLD {
            continue;
        }

        let shape = mask.size().unwrap();
        // SAFETY: The position (row, col) is guaranteed by OpenCV
        let x = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_LEFT).unwrap() };
        let y = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_TOP).unwrap() };
        let w = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_WIDTH).unwrap() };
        let h = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_HEIGHT).unwrap() };
        let size = area as f64 * w.min(h) as f64 / (w as f64 * h as f64);
        let size = ((size).sqrt() * 2.0) as i32;
        let sx = (x - size + 1).max(0);
        let sy = (y - size + 1).max(0);
        let ex = (x + w + size + 1).min(shape.width);
        let ey = (y + h + size + 1).min(shape.height);
        let kernel =
            get_structuring_element_def(MORPH_RECT, Size::new(size + 1, size + 1)).unwrap();

        let mut link_mask = Mat::default();
        let mut text_mask = Mat::default();
        let mut and_mask = Mat::default();
        let mut seg_map = Mat::zeros(shape.height, shape.width, CV_8U)
            .unwrap()
            .to_mat()
            .unwrap();
        compare(&link_score, &Scalar::all(1.0), &mut link_mask, CMP_EQ).unwrap();
        compare(&text_score, &Scalar::all(0.0), &mut text_mask, CMP_EQ).unwrap();
        bitwise_and_def(&link_mask, &text_mask, &mut and_mask).unwrap();
        seg_map.set_to(&Scalar::all(255.0), &mask).unwrap();
        seg_map.set_to(&Scalar::all(0.0), &and_mask).unwrap();

        let mut seg_contours = Vector::<Point>::new();
        let mut seg_roi = seg_map
            .roi_mut(Rect::from_points(Point::new(sx, sy), Point::new(ex, ey)))
            .unwrap();
        // SAFETY: all of the functions below can be called in place.
        unsafe {
            seg_roi.modify_inplace(|mat, mat_mut| {
                dilate_def(mat, mat_mut, &kernel).unwrap();
                mat.copy_to(mat_mut).unwrap();
            });
        }
        find_non_zero(&seg_map, &mut seg_contours).unwrap();

        let contour = min_area_rect(&seg_contours)
            .unwrap()
            .bounding_rect2f()
            .unwrap();
        let tl = contour.tl();
        let tl = Point::new(
            (tl.x * w_ratio * 2.0) as i32 + x_offset,
            (tl.y * h_ratio * 2.0) as i32 + y_offset,
        );
        let br = contour.br();
        let br = Point::new(
            (br.x * w_ratio * 2.0) as i32 + x_offset,
            (br.y * h_ratio * 2.0) as i32 + y_offset,
        );
        bboxes.push(Rect::from_points(tl, br));
    }
    bboxes
}

#[inline]
fn remap_from_yolo(pred: &[f32], size: Size, w_ratio: f32, h_ratio: f32) -> Rect {
    let tl_x = (pred[0] * w_ratio).max(0.0).min(size.width as f32);
    let tl_y = (pred[1] * h_ratio).max(0.0).min(size.height as f32);
    let br_x = (pred[2] * w_ratio).max(0.0).min(size.width as f32);
    let br_y = (pred[3] * h_ratio).max(0.0).min(size.height as f32);
    Rect::from_points(
        Point::new(tl_x as i32, tl_y as i32),
        Point::new(br_x as i32, br_y as i32),
    )
}

/// Preprocesses a BGRA `Mat` image to a normalized and resized RGB `Mat` image with type `f32`
/// for YOLO detection.
///
/// Returns a triplet of `(Mat, width_ratio, height_ratio)` with the ratios calculed from
/// `old_size / new_size`.
#[inline]
fn preprocess_for_yolo(mat: &impl MatTraitConst) -> (Mat, f32, f32) {
    let mut mat = mat.try_clone().unwrap();
    let (w_ratio, h_ratio) = resize_w_h_ratio(mat.size().unwrap(), 640.0, 640.0);
    // SAFETY: all of the functions below can be called in place.
    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize(mat, mat_mut, Size::new(640, 640), 0.0, 0.0, INTER_AREA).unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0 / 255.0, 0.0).unwrap();
        });
    }
    (mat, w_ratio, h_ratio)
}

/// Preprocesses a BGRA `Mat` image to a normalized and resized RGB `Mat` image with type `f32`
/// for text bounding boxes detection.
///
/// The preprocess is adapted from: https://github.com/clovaai/CRAFT-pytorch/blob/master/imgproc.py
///
/// Returns a `(Mat, width_ratio, height_ratio)`.
#[inline]
fn preprocess_for_text_bboxes(mat: &impl MatTraitConst) -> (Mat, f32, f32) {
    let mut mat = mat.try_clone().unwrap();
    let size = mat.size().unwrap();
    let size_w = size.width as f32;
    let size_h = size.height as f32;
    let size_max = size_w.max(size_h);
    let resize_size = 5.0 * size_max;
    let resize_ratio = resize_size / size_max;

    let resize_w = (resize_ratio * size_w) as i32;
    let resize_w = (resize_w + 31) & !31; // rounds to multiple of 32
    let resize_w_ratio = size_w / resize_w as f32;

    let resize_h = (resize_ratio * size_h) as i32;
    let resize_h = (resize_h + 31) & !31;
    let resize_h_ratio = size_h / resize_h as f32;
    // SAFETY: all of the below functions can be called in place
    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize(
                mat,
                mat_mut,
                Size::new(resize_w, resize_h),
                0.0,
                0.0,
                INTER_CUBIC,
            )
            .unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0, 0.0).unwrap();
            // these values are pre-multiplied from the above link in normalizeMeanVariance
            subtract_def(mat, &Scalar::new(123.675, 116.28, 103.53, 0.0), mat_mut).unwrap();
            divide2_def(&mat, &Scalar::new(58.395, 57.12, 57.375, 1.0), mat_mut).unwrap();
        });
    }
    (mat, resize_w_ratio, resize_h_ratio)
}

/// Retrieves `(width, height)` ratios for resizing.
#[inline]
fn resize_w_h_ratio(from: Size, to_w: f32, to_h: f32) -> (f32, f32) {
    (from.width as f32 / to_w, from.height as f32 / to_h)
}

/// Converts an BGRA `Mat` image to BGR.
#[inline]
fn to_bgr(mat: &impl MatTraitConst) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: can be modified inplace
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2BGR).unwrap();
        });
    }
    mat
}

/// Converts an BGRA `Mat` image to grayscale.
///
/// `add_contrast` can be set to `true` in order to increase contrast by a fixed amount
/// used for template matching.
#[inline]
fn to_grayscale(mat: &impl MatTraitConst, add_contrast: bool) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: all of the functions below can be called in place.
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2GRAY).unwrap();
            if add_contrast {
                // TODO: is this needed?
                add_weighted_def(mat, 1.5, mat, 0.0, -80.0, mat_mut).unwrap();
            }
        });
    }
    mat
}

/// Extracts a borrowed `Mat` from `SessionOutputs`.
///
/// The returned `BoxedRef<'_, Mat>` has shape `[..dims]` with batch size (1) removed.
#[inline]
fn from_output_value<'a>(result: &SessionOutputs) -> BoxedRef<'a, Mat> {
    let (dims, outputs) = result["output0"].try_extract_raw_tensor::<f32>().unwrap();
    let dims = dims.iter().map(|&dim| dim as i32).collect::<Vec<i32>>();
    let mat = Mat::new_nd_with_data(dims.as_slice(), outputs).unwrap();
    let mat = mat.reshape_nd(1, &dims.as_slice()[1..]).unwrap();
    let mat = mat.opencv_into_extern_container_nofail();
    BoxedRef::from(mat)
}

/// Converts a continuous, normalized `f32` RGB `Mat` image to `SessionInputValue`.
///
/// The input `Mat` is assumed to be continuous, normalized RGB `f32` data type and
/// will panic if not. The `Mat` is reshaped to single channel, tranposed to `[1, 3, H, W]` and
/// converted to `SessionInputValue`.
#[inline]
fn norm_rgb_to_input_value(mat: &impl MatTraitConst) -> SessionInputValue {
    let mat = mat.reshape_nd(1, &[1, mat.rows(), mat.cols(), 3]).unwrap();
    let mut mat_t = Mat::default();
    transpose_nd(&mat, &Vector::from_slice(&[0, 3, 1, 2]), &mut mat_t).unwrap();
    let shape = mat_t.mat_size();
    let input = (shape.as_slice(), mat_t.data_typed::<f32>().unwrap());
    let tensor = Tensor::from_array(input).unwrap();
    SessionInputValue::Owned(tensor.into_dyn())
}
