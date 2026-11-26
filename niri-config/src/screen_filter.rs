use knuffel::errors::DecodeError;

use crate::animations::parse_custom_shader_path;
use crate::utils::parse_arg_node;

/// A display-only shader, applied before the optional grayscale conversion.
#[derive(knuffel::Decode, Debug, Default, Clone, PartialEq, Eq)]
pub struct ScreenFilter {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(child)]
    pub custom_shader: Option<CustomShader>,
}

impl ScreenFilter {
    pub fn shader_source(&self) -> Option<&str> {
        self.custom_shader.as_ref().map(|shader| shader.0.as_str())
    }

    pub fn is_enabled(&self) -> bool {
        !self.off && self.custom_shader.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomShader(pub String);

impl<S: knuffel::traits::ErrorSpan> knuffel::Decode<S> for CustomShader {
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        let source = if node.properties.contains_key("path") {
            parse_custom_shader_path(node, ctx)?
        } else {
            parse_arg_node("custom-shader", node, ctx)?
        };
        Ok(Self(source))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{Action, BoundAction, Config};

    #[test]
    fn parse_screen_filter() {
        assert!(!Config::default().screen_filter.is_enabled());
        let config = Config::parse_mem(
            r#"
            screen-filter { custom-shader "vec4 filter_color(vec4 c, vec2 uv) { return c; }"; }
            binds { Mod+Shift+S { toggle-screen-filter; }; }
            "#,
        )
        .unwrap();
        assert!(config.screen_filter.is_enabled());
        assert!(config
            .screen_filter
            .shader_source()
            .unwrap()
            .contains("filter_color"));
        assert_eq!(
            config.binds.0[0].action,
            BoundAction::Press(Action::ToggleScreenFilter)
        );
        assert_eq!(
            Action::from(niri_ipc::Action::ToggleScreenFilter {}),
            Action::ToggleScreenFilter
        );
        let config =
            Config::parse_mem(r#"screen-filter { off; custom-shader "shader"; }"#).unwrap();
        assert!(!config.screen_filter.is_enabled());
        assert_eq!(config.screen_filter.shader_source(), Some("shader"));

        for text in [
            r#"screen-filter { custom-shader; }"#,
            r#"screen-filter { custom-shader "one"; custom-shader "two"; }"#,
            r#"screen-filter { unknown; }"#,
            r#"screen-filter "unexpected""#,
            r#"screen-filter { custom-shader path="shader" "unexpected"; }"#,
        ] {
            assert!(Config::parse_mem(text).is_err(), "{text}");
        }
    }

    #[test]
    fn shader_paths_are_relative_and_watched_even_when_missing() {
        let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("config.kdl");
        let shader_path = config_path
            .parent()
            .unwrap()
            .join("../resources/screen-filters/invert.frag");
        let parsed = Config::parse(
            &config_path,
            r#"screen-filter { custom-shader path="../resources/screen-filters/invert.frag"; }"#,
        );
        assert_eq!(
            parsed.includes.as_slice(),
            std::slice::from_ref(&shader_path)
        );
        assert_eq!(
            parsed.config.unwrap().screen_filter.shader_source(),
            Some(std::fs::read_to_string(shader_path).unwrap().as_str())
        );

        let parsed = Config::parse(
            &config_path,
            r#"screen-filter { custom-shader path="missing-filter.frag"; }"#,
        );
        assert!(parsed.config.is_err());
        assert_eq!(
            parsed.includes,
            [config_path.parent().unwrap().join("missing-filter.frag")]
        );
    }
}
