
void main() {
    vec2 coords = (niri_input_to_geo * vec3(v_coords, 1.0)).xy;
    vec4 color = texture2D(tex, v_coords);
#if defined(NO_ALPHA)
    color.a = 1.0;
#endif

    color = filter_color(color, coords);
    if (niri_grayscale == 1.0) {
        color.rgb = vec3(dot(color.rgb, vec3(0.2126, 0.7152, 0.0722)));
    }
    color *= alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif
    gl_FragColor = color;
}
