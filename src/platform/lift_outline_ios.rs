//! The outline the system's context menu lifts an account in (iOS). NSH lifts the menu's trigger slot, a plain view
//! around the row whose layer has no corners of its own, so the row would lift square with the system's platter in its
//! corners. This component, placed inside the row with the row's corners as its props (`top`, `bottom`), gives that
//! view an unfilled shape layer of the row's outline behind its content: the outline NSH reads for the lifted preview
//! (`nsh-platform-ios` `decor::outline`; recheck on every NSH upgrade). It draws nothing and takes no touches.
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, NSObject},
};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_ui_kit::{UIResponder, UIView};
use std::cell::Cell;

/// A `CGPathRef` or `CGColorRef` as Objective-C methods take and return it (the encoding objc2 checks).
#[repr(transparent)]
#[derive(Clone, Copy)]
struct CgRef<const COLOR: bool>(*const std::ffi::c_void);
// SAFETY: a pointer to the opaque Core Graphics struct its method signatures name.
unsafe impl objc2::encode::Encode for CgRef<false> {
    const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Struct("CGPath", &[]));
}
// SAFETY: as above.
unsafe impl objc2::encode::Encode for CgRef<true> {
    const ENCODING: objc2::encode::Encoding = objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Struct("CGColor", &[]));
}

/// The native component: place it with `native(OUTLINE)` inside the row.
pub const OUTLINE: &str = "safay.lift-outline";

/// The layer's name on the trigger's view, to find it again.
const NAME: &str = "safay.lift-outline";
/// NSH's depth for a view's own background layers, which its outline reads (`decor::BEHIND`).
const BEHIND: f64 = -1024.0;

#[derive(Default)]
struct Ivars {
    top: Cell<f64>,
    bottom: Cell<f64>,
}

define_class!(
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    #[name = "SafayLiftOutline"]
    struct OutlineView;

    impl OutlineView {
        #[unsafe(method(layoutSubviews))]
        fn layout_subviews(&self) {
            let _: () = unsafe { msg_send![super(self), layoutSubviews] };
            self.apply();
        }

        #[unsafe(method(didMoveToWindow))]
        fn did_move_to_window(&self) {
            let _: () = unsafe { msg_send![super(self), didMoveToWindow] };
            self.apply();
        }
    }
);

impl OutlineView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        let view: Retained<Self> = unsafe { msg_send![super(this), init] };
        view.setUserInteractionEnabled(false);
        view
    }

    /// The trigger's view: the first view up from here holding only the row, at the row's size.
    fn trigger(&self) -> Option<Retained<UIView>> {
        let mut row = self.superview()?;
        for _ in 0..6 {
            let up = row.superview()?;
            if up.subviews().count() == 1 && up.bounds().size == row.bounds().size && row.subviews().count() > 1 {
                return Some(up);
            }
            row = up;
        }
        None
    }

    fn apply(&self) {
        let Some(view) = self.trigger() else { return };
        let layer = view.layer();
        let name = objc2_foundation::NSString::from_str(NAME);
        // SAFETY: CALayer's `sublayers`, `name`, and CAShapeLayer's setters, on the main thread; `CGPath` is owned by
        // the bezier path, which lives until the shape layer has copied it.
        unsafe {
            let sublayers: *mut AnyObject = msg_send![&*layer, sublayers];
            let mut shape: Option<Retained<AnyObject>> = None;
            if let Some(list) = sublayers.as_ref() {
                let count: usize = msg_send![list, count];
                for i in 0..count {
                    let l: *mut AnyObject = msg_send![list, objectAtIndex: i];
                    let n: *mut AnyObject = msg_send![l, name];
                    if n.as_ref().is_some_and(|n| {
                        let same: bool = msg_send![n, isEqualToString: &*name];
                        same
                    }) {
                        shape = Retained::retain(l);
                        break;
                    }
                }
            }
            let shape = match shape {
                Some(shape) => shape,
                None => {
                    let Some(class) = AnyClass::get(c"CAShapeLayer") else { return };
                    let shape: Retained<AnyObject> = msg_send![class, layer];
                    let _: () = msg_send![&*shape, setName: &*name];
                    let _: () = msg_send![&*shape, setZPosition: BEHIND];
                    let _: () = msg_send![&*shape, setFillColor: CgRef::<true>(std::ptr::null())];
                    let _: () = msg_send![&*layer, insertSublayer: &*shape, atIndex: 0u32];
                    shape
                }
            };
            let bounds: CGRect = view.bounds();
            let _: () = msg_send![&*shape, setFrame: bounds];
            let Some(path) = rounded(bounds, self.ivars().top.get(), self.ivars().bottom.get()) else { return };
            let cg: CgRef<false> = msg_send![&*path, CGPath];
            let _: () = msg_send![&*shape, setPath: cg];
        }
    }
}

/// A bezier path of `bounds`' outline, its top corners `top`, its bottom ones `bottom`.
fn rounded(bounds: CGRect, top: f64, bottom: f64) -> Option<Retained<AnyObject>> {
    use std::f64::consts::{FRAC_PI_2, PI};
    let (w, h) = (bounds.size.width, bounds.size.height);
    let t = top.min(w / 2.0).min(h / 2.0).max(0.0);
    let b = bottom.min(w / 2.0).min(h / 2.0).max(0.0);
    let point = |x: f64, y: f64| CGPoint { x, y };
    // SAFETY: UIBezierPath's building methods on a path made here.
    unsafe {
        let class = AnyClass::get(c"UIBezierPath")?;
        let p: Retained<AnyObject> = msg_send![class, bezierPath];
        let arc = |cx: f64, cy: f64, r: f64, from: f64, to: f64| {
            let _: () = msg_send![&*p, addArcWithCenter: point(cx, cy), radius: r, startAngle: from, endAngle: to, clockwise: true];
        };
        let _: () = msg_send![&*p, moveToPoint: point(t, 0.0)];
        let _: () = msg_send![&*p, addLineToPoint: point(w - t, 0.0)];
        arc(w - t, t, t, -FRAC_PI_2, 0.0);
        let _: () = msg_send![&*p, addLineToPoint: point(w, h - b)];
        arc(w - b, h - b, b, 0.0, FRAC_PI_2);
        let _: () = msg_send![&*p, addLineToPoint: point(b, h)];
        arc(b, h - b, b, FRAC_PI_2, PI);
        let _: () = msg_send![&*p, addLineToPoint: point(0.0, t)];
        arc(t, t, t, PI, PI + FRAC_PI_2);
        let _: () = msg_send![&*p, closePath];
        Some(p)
    }
}

/// Registers [`OUTLINE`].
pub fn register(components: &nsh_platform_ios::Components) {
    components.register(
        OUTLINE,
        std::rc::Rc::new(|_context: nsh_platform_ios::ComponentContext| -> Box<dyn nsh_platform_ios::NativeComponent> {
            let mtm = MainThreadMarker::new().expect("components are made on the main thread");
            Box::new(Outline { view: OutlineView::new(mtm) })
        }),
    );
}

struct Outline {
    view: Retained<OutlineView>,
}

impl nsh_platform_ios::NativeComponent for Outline {
    fn view(&self) -> &UIView {
        &self.view
    }

    fn set_prop(&mut self, name: &str, value: Option<&nsh_core::JsonValue>) {
        let value = value.and_then(nsh_core::JsonValue::as_f64).unwrap_or(0.0);
        match name {
            "top" => self.view.ivars().top.set(value),
            "bottom" => self.view.ivars().bottom.set(value),
            _ => return,
        }
        self.view.apply();
    }
}
