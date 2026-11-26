use niri_config::{Action, Config};
use smithay::backend::allocator::Fourcc;
use smithay::backend::input::InputTime;
use smithay::backend::renderer::element::Element as _;
use smithay::output::Scale;
use smithay::utils::Transform;

use super::Fixture;
use crate::render_helpers::{render_to_vec, RenderCtx, RenderTarget};

#[test]
fn toggle_and_config_reload() {
    let mut f = Fixture::new();
    f.add_output(1, (1920, 1080));
    f.add_output(2, (1280, 720));
    assert!(!f.niri().config.borrow().grayscale);

    f.niri_state()
        .do_action(Action::ToggleGrayscale, false, InputTime::now());
    assert!(f.niri().config.borrow().grayscale);
    f.niri_state()
        .do_action(Action::ToggleGrayscale, false, InputTime::now());
    assert!(!f.niri().config.borrow().grayscale);

    f.niri_state()
        .reload_config(Ok(Config::parse_mem("grayscale").unwrap()));
    assert!(f.niri().config.borrow().grayscale);
    f.niri_state().reload_config(Ok(Config::default()));
    assert!(!f.niri().config.borrow().grayscale);
}

#[test]
fn egl_grayscale_filters_only_outputs() {
    let config = Config::parse_mem(
        r##"
        layout { background-color "#ff0000"; }
        hotkey-overlay { skip-at-startup; }
        animations { off; }
        "##,
    )
    .unwrap();
    let mut f = Fixture::with_config(config);
    f.niri_state().backend.headless().add_renderer().unwrap();
    f.add_output(1, (80, 60));
    f.add_output(2, (100, 80));
    let outputs = [f.niri_output(1), f.niri_output(2)];
    outputs[1].change_current_state(
        None,
        Some(Transform::_90),
        Some(Scale::Fractional(1.25)),
        None,
    );
    f.niri().reposition_outputs(None);

    // Exercise inserting and removing the effect on otherwise unchanged contents.
    for enabled in [false, true, false] {
        f.niri().config.borrow_mut().grayscale = enabled;
        f.niri().update_render_elements(None);
        let state = f.niri_state();
        state
            .backend
            .with_primary_renderer(|renderer| {
                if enabled {
                    // Disabling grayscale during a transition must restore color immediately,
                    // rather than leaving grayscale baked into the frozen snapshot.
                    state.niri.do_screen_transition(renderer, Some(1000));
                }
                for output in &outputs {
                    for target in [
                        RenderTarget::Output,
                        RenderTarget::Screencast,
                        RenderTarget::ScreenCapture,
                    ] {
                        let ctx = RenderCtx {
                            renderer,
                            target,
                            xray: None,
                        };
                        let elements = state.niri.render_to_vec(ctx, output, false);
                        let grayscale = enabled && target == RenderTarget::Output;
                        assert_eq!(elements[0].is_framebuffer_effect(), grayscale);
                        let pixels = render_to_vec(
                            renderer,
                            output
                                .current_mode()
                                .unwrap()
                                .size
                                .to_logical(1)
                                .to_physical(1),
                            output.current_scale().fractional_scale().into(),
                            output.current_transform(),
                            Fourcc::Abgr8888,
                            elements.iter().rev(),
                        )
                        .unwrap();
                        for pixel in pixels.chunks_exact(4) {
                            if grayscale {
                                assert_eq!(pixel[0], pixel[1]);
                                assert_eq!(pixel[1], pixel[2]);
                                assert!(pixel[0] > 0 && pixel[0] < 255);
                            } else {
                                assert_eq!(&pixel[..3], &[255, 0, 0]);
                            }
                            assert_eq!(pixel[3], 255);
                        }
                    }
                }
            })
            .unwrap();
    }
}
