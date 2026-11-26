// Display-only color inversion. No main(), #version, or uniform declarations needed.
vec4 filter_color(vec4 color, vec2 coords) {
    return vec4(vec3(color.a) - color.rgb, color.a);
}
