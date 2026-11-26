use niri_config::{Action, Config};
use smithay::backend::allocator::Fourcc;
use smithay::backend::input::InputTime;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::{Bind as _, Color32F, ExportMem as _, Offscreen as _};
use smithay::output::{Output, Scale};
use smithay::utils::Transform;
use xshell::Shell;

use super::Fixture;
use crate::ipc::server::perform_action;
use crate::render_helpers::solid_color::{SolidColorBuffer, SolidColorRenderElement};
use crate::render_helpers::{copy_framebuffer, render_to_vec, RenderCtx, RenderTarget};
use crate::utils::output_size;

const INVERT: &str = include_str!("../../resources/screen-filters/invert.frag");

fn config(shader: Option<&str>) -> Config {
    let mut config = Config::parse_mem(
        r##"
        layout { background-color "#ff0000"; }
        animations { off; }
        hotkey-overlay { skip-at-startup; }
        config-notification { disable-failed; }
        "##,
    )
    .unwrap();
    config.screen_filter.custom_shader =
        shader.map(|source| niri_config::screen_filter::CustomShader {
            source: source.to_owned(),
            path: None,
        });
    config
}

fn action(f: &mut Fixture, action: Action) {
    f.niri_state().do_action(action, false, InputTime::now());
}

fn pixels(f: &mut Fixture, output: &Output, target: RenderTarget) -> Vec<u8> {
    f.niri().update_render_elements(None);
    let state = f.niri_state();
    state
        .backend
        .with_primary_renderer(|renderer| {
            let ctx = RenderCtx {
                renderer,
                target,
                xray: None,
            };
            let elements = state.niri.render_to_vec(ctx, output, false);
            render_to_vec(
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
            .unwrap()
        })
        .unwrap()
}

// Reuse both the framebuffer and damage tracker to catch stale pixels on shader edits
// and toggles, rather than forcing a full repaint on every test frame.
fn cached_pixels(
    f: &mut Fixture,
    output: &Output,
    texture: &mut GlesTexture,
    damage: &mut OutputDamageTracker,
) -> Vec<u8> {
    f.niri().update_render_elements(None);
    let state = f.niri_state();
    state
        .backend
        .with_primary_renderer(|renderer| {
            let ctx = RenderCtx {
                renderer,
                target: RenderTarget::Output,
                xray: None,
            };
            let elements = state.niri.render_to_vec(ctx, output, false);
            let mut target = renderer.bind(texture).unwrap();
            damage
                .render_output(renderer, &mut target, 1, &elements, Color32F::TRANSPARENT)
                .unwrap();
            let mapping = copy_framebuffer(renderer, &target, Fourcc::Abgr8888).unwrap();
            renderer.map_texture(&mapping).unwrap().to_vec()
        })
        .unwrap()
}

fn assert_color(pixels: &[u8], color: [u8; 4]) {
    assert!(!pixels.is_empty());
    for pixel in pixels.chunks_exact(4) {
        assert_eq!(pixel, color);
    }
}

#[test]
fn toggle_and_config_reload() {
    let mut f = Fixture::with_config(config(Some(INVERT)));
    f.add_output(1, (80, 60));
    f.add_output(2, (100, 80));
    assert!(f.niri().config.borrow().screen_filter.is_enabled());
    action(&mut f, Action::ToggleScreenFilter);
    assert!(!f.niri().config.borrow().screen_filter.is_enabled());
    f.niri_state().reload_config(Ok(config(Some(INVERT))));
    assert!(f.niri().config.borrow().screen_filter.is_enabled());

    let mut disabled = config(Some(INVERT));
    disabled.screen_filter.off = true;
    f.niri_state().reload_config(Ok(disabled));
    assert!(!f.niri().config.borrow().screen_filter.is_enabled());
    action(&mut f, Action::ToggleScreenFilter);
    assert!(f.niri().config.borrow().screen_filter.is_enabled());
    f.niri_state().reload_config(Ok(config(None)));
    assert!(!f.niri().config.borrow().screen_filter.is_enabled());
    // A custom-filter toggle never changes the independent grayscale option.
    action(&mut f, Action::ToggleGrayscale);
    action(&mut f, Action::ToggleScreenFilter);
    assert!(f.niri().config.borrow().grayscale);
}

#[test]
fn runtime_shader_errors_leave_config_unchanged_without_a_renderer() {
    let sh = Shell::new().unwrap();
    let dir = sh.create_temp_dir().unwrap();
    let path = dir.path().join("filter.frag");
    sh.write_file(&path, INVERT).unwrap();
    let mut f = Fixture::with_config(config(None));
    let error = perform_action(f.niri_state(), Action::ReloadScreenFilter).unwrap_err();
    assert!(error.contains("no screen filter is configured"), "{error}");
    let error = perform_action(
        f.niri_state(),
        Action::SetScreenFilter(path.to_str().unwrap().into()),
    )
    .unwrap_err();
    assert!(error.contains("no renderer"), "{error}");
    assert!(f
        .niri()
        .config
        .borrow()
        .screen_filter
        .custom_shader
        .is_none());
}

#[test]
fn egl_runtime_shader_selection_and_explicit_reload_are_atomic() {
    let sh = Shell::new().unwrap();
    let dir = sh.create_temp_dir().unwrap();
    let path = dir.path().join("filter.frag");
    let other = dir.path().join("other.frag");
    sh.write_file(&path, INVERT).unwrap();
    sh.write_file(&other, "invalid GLSL").unwrap();
    let mut f = Fixture::with_config(config(None));
    f.niri_state().backend.headless().add_renderer().unwrap();
    f.add_output(1, (80, 60));
    let output = f.niri_output(1);
    let mut damage = OutputDamageTracker::from_output(&output);
    let mut texture = f
        .niri_state()
        .backend
        .with_primary_renderer(|renderer| {
            renderer
                .create_buffer(Fourcc::Abgr8888, (80, 60).into())
                .unwrap()
        })
        .unwrap();
    f.niri().config.borrow_mut().screen_filter.off = true;
    perform_action(
        f.niri_state(),
        Action::SetScreenFilter(path.to_str().unwrap().into()),
    )
    .unwrap();
    assert!(f.niri().config.borrow().screen_filter.is_enabled());
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 255, 255, 255],
    );

    let blue = "vec4 filter_color(vec4 c, vec2 uv) { return vec4(0., 0., 1., 1.); }";
    sh.write_file(&path, blue).unwrap();
    perform_action(f.niri_state(), Action::ReloadScreenFilter).unwrap();
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 0, 255, 255],
    );
    for target in [RenderTarget::ScreenCapture, RenderTarget::Screencast] {
        assert_color(&pixels(&mut f, &output, target), [255, 0, 0, 255]);
    }

    // Reload preserves the off state, but selecting a file explicitly enables it.
    action(&mut f, Action::ToggleScreenFilter);
    sh.write_file(&path, INVERT).unwrap();
    perform_action(f.niri_state(), Action::ReloadScreenFilter).unwrap();
    assert!(!f.niri().config.borrow().screen_filter.is_enabled());
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [255, 0, 0, 255],
    );
    action(&mut f, Action::ToggleScreenFilter);
    let selected = f.niri().config.borrow().screen_filter.clone();

    // A missing/invalid file must return an IPC error without changing either selection or pixels.
    for candidate in [other.clone(), dir.path().join("missing.frag")] {
        assert!(perform_action(
            f.niri_state(),
            Action::SetScreenFilter(candidate.to_str().unwrap().into())
        )
        .is_err());
        assert_eq!(f.niri().config.borrow().screen_filter, selected);
    }
    sh.remove_path(&path).unwrap();
    assert!(perform_action(f.niri_state(), Action::ReloadScreenFilter).is_err());
    assert_eq!(f.niri().config.borrow().screen_filter, selected);
    sh.write_file(&path, "invalid GLSL").unwrap();
    assert!(perform_action(f.niri_state(), Action::ReloadScreenFilter).is_err());
    assert_eq!(f.niri().config.borrow().screen_filter, selected);
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 255, 255, 255],
    );

    sh.write_file(&other, blue).unwrap();
    perform_action(
        f.niri_state(),
        Action::SetScreenFilter(other.to_str().unwrap().into()),
    )
    .unwrap();
    assert_eq!(
        f.niri()
            .config
            .borrow()
            .screen_filter
            .custom_shader
            .as_ref()
            .unwrap()
            .path,
        Some(other)
    );
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 0, 255, 255],
    );

    // Reload of an inline shader recompiles it; a config reload resets the transient selection.
    f.niri_state().reload_config(Ok(config(Some(INVERT))));
    perform_action(f.niri_state(), Action::ReloadScreenFilter).unwrap();
    assert!(f
        .niri()
        .config
        .borrow()
        .screen_filter
        .custom_shader
        .as_ref()
        .unwrap()
        .path
        .is_none());
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 255, 255, 255],
    );
    f.niri_state().reload_config(Ok(config(None)));
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [255, 0, 0, 255],
    );
}

#[test]
fn egl_toggle_reload_damage_and_bad_shader_fallback() {
    let mut f = Fixture::with_config(config(None));
    f.niri_state().backend.headless().add_renderer().unwrap();
    f.add_output(1, (80, 60));
    let output = f.niri_output(1);
    let mut damage = OutputDamageTracker::from_output(&output);
    let mut texture = f
        .niri_state()
        .backend
        .with_primary_renderer(|renderer| {
            renderer
                .create_buffer(Fourcc::Abgr8888, (80, 60).into())
                .unwrap()
        })
        .unwrap();

    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [255, 0, 0, 255],
    );
    f.niri_state().reload_config(Ok(config(Some(INVERT))));
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 255, 255, 255],
    );
    for target in [RenderTarget::Screencast, RenderTarget::ScreenCapture] {
        assert_color(&pixels(&mut f, &output, target), [255, 0, 0, 255]);
    }
    action(&mut f, Action::ToggleScreenFilter);
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [255, 0, 0, 255],
    );
    action(&mut f, Action::ToggleScreenFilter);
    action(&mut f, Action::ToggleGrayscale);
    let gray = cached_pixels(&mut f, &output, &mut texture, &mut damage);
    for pixel in gray.chunks_exact(4) {
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
        // Cyan after custom inversion, not red before it.
        assert!((199..=202).contains(&pixel[0]));
    }
    action(&mut f, Action::ToggleGrayscale);
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 255, 255, 255],
    );
    let blue = "vec4 filter_color(vec4 c, vec2 uv) { return vec4(0., 0., 1., 1.); }";
    f.niri_state().reload_config(Ok(config(Some(blue))));
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 0, 255, 255],
    );
    f.niri_state()
        .reload_config(Ok(config(Some("invalid GLSL"))));
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [0, 0, 255, 255],
    );
    f.niri_state().reload_config(Ok(config(None)));
    assert_color(
        &cached_pixels(&mut f, &output, &mut texture, &mut damage),
        [255, 0, 0, 255],
    );
}

#[test]
fn egl_invalid_initial_shader_and_transition_snapshot() {
    let mut f = Fixture::with_config(config(None));
    f.niri_state().backend.headless().add_renderer().unwrap();
    f.add_output(1, (80, 60));
    let output = f.niri_output(1);
    f.niri_state()
        .reload_config(Ok(config(Some("invalid GLSL"))));
    assert_color(
        &pixels(&mut f, &output, RenderTarget::Output),
        [255, 0, 0, 255],
    );
    f.niri_state().reload_config(Ok(config(Some(INVERT))));
    let state = f.niri_state();
    state
        .backend
        .with_primary_renderer(|renderer| {
            state.niri.do_screen_transition(renderer, Some(1000));
        })
        .unwrap();
    assert_color(
        &pixels(&mut f, &output, RenderTarget::Output),
        [0, 255, 255, 255],
    );
    action(&mut f, Action::ToggleScreenFilter);
    assert_color(
        &pixels(&mut f, &output, RenderTarget::Output),
        [255, 0, 0, 255],
    );
}

#[test]
fn egl_coordinates_sampling_and_size_across_output_transforms() {
    let shader = r#"
        vec4 filter_color(vec4 color, vec2 coords) {
            if (min(niri_size.x, niri_size.y) != 60. || max(niri_size.x, niri_size.y) != 80.)
                return vec4(0., 0., 1., 1.);
            if (coords.x < 0.5 && coords.y < 0.5)
                return vec4(0., 1., 0., 1.);
            return niri_sample(coords);
        }
    "#;
    let mut f = Fixture::with_config(config(None));
    f.niri_state().backend.headless().add_renderer().unwrap();
    f.add_output(1, (80, 60));
    let output = f.niri_output(1);
    f.niri_state().reload_config(Ok(config(Some(shader))));

    for transform in [
        Transform::Normal,
        Transform::_90,
        Transform::_180,
        Transform::_270,
        Transform::Flipped,
        Transform::Flipped90,
        Transform::Flipped180,
        Transform::Flipped270,
    ] {
        for scale in [1., 1.25, 2.] {
            output.change_current_state(
                None,
                Some(transform),
                Some(Scale::Fractional(scale)),
                None,
            );
            f.niri().reposition_outputs(None);
            // Compare the shader's top-left green quadrant against a real element placed
            // at that location. An asymmetric blue patch in the source also checks that
            // niri_sample() preserves the underlying image across transforms and scales.
            let size = output_size(&output);
            let green = SolidColorBuffer::new((size.w / 2., size.h / 2.), [0., 1., 0., 1.]);
            let green =
                SolidColorRenderElement::from_buffer(&green, (0., 0.), 1., Kind::Unspecified);
            let blue = SolidColorBuffer::new((size.w / 2., size.h / 2.), [0., 0., 1., 1.]);
            let blue = SolidColorRenderElement::from_buffer(
                &blue,
                (size.w / 2., size.h / 2.),
                1.,
                Kind::Unspecified,
            );
            f.niri().update_render_elements(None);
            let state = f.niri_state();
            let (filtered, reference) = state
                .backend
                .with_primary_renderer(|renderer| {
                    let ctx = RenderCtx {
                        renderer,
                        target: RenderTarget::Output,
                        xray: None,
                    };
                    let mut elements = state.niri.render_to_vec(ctx, &output, false);
                    elements.insert(1, blue.clone().into());
                    let filtered = render_to_vec(
                        renderer,
                        (80, 60).into(),
                        scale.into(),
                        transform,
                        Fourcc::Abgr8888,
                        elements.iter().rev(),
                    )
                    .unwrap();
                    let ctx = RenderCtx {
                        renderer,
                        target: RenderTarget::ScreenCapture,
                        xray: None,
                    };
                    let mut elements = state.niri.render_to_vec(ctx, &output, false);
                    elements.insert(0, blue.into());
                    elements.insert(0, green.into());
                    let reference = render_to_vec(
                        renderer,
                        (80, 60).into(),
                        scale.into(),
                        transform,
                        Fourcc::Abgr8888,
                        elements.iter().rev(),
                    )
                    .unwrap();
                    (filtered, reference)
                })
                .unwrap();
            assert_eq!(filtered.len(), reference.len());
            for (index, (actual, expected)) in filtered
                .chunks_exact(4)
                .zip(reference.chunks_exact(4))
                .enumerate()
            {
                assert_eq!(
                    actual, expected,
                    "{transform:?}, scale {scale}, pixel {index}"
                );
            }
        }
    }
}
