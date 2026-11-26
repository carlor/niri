#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

// Output-relative coordinates are normalized, with (0, 0) at the top-left,
// regardless of output transform. niri_size is the visible size in physical pixels.
uniform vec2 niri_size;
uniform mat3 niri_input_to_geo;
uniform mat3 niri_geo_to_tex;
uniform float niri_grayscale;

// Sample the unfiltered scene at output-relative normalized coordinates.
vec4 niri_sample(vec2 coords) {
    return texture2D(tex, (niri_geo_to_tex * vec3(coords, 1.0)).xy);
}

vec4 filter_color(vec4 color, vec2 coords);
