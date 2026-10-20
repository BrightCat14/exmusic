#version 440

precision highp float;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    highp vec2 resolution;
    highp float time;
    highp vec4 audio;
    highp vec4 paletteTop;
    highp vec4 paletteMid;
    highp vec4 paletteBottom;
    highp vec4 paletteGlow;
};

layout(location = 0) in highp vec2 qt_TexCoord0;
layout(location = 0) out highp vec4 fragColor;

const float PI = 3.14159265359;

float hash21(vec2 p) {
    p = fract(p * vec2(123.34, 345.45));
    p += dot(p, p + 34.345);
    return fract(p.x * p.y);
}

float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return mix(mix(hash21(i), hash21(i + vec2(1.0, 0.0)), f.x),
               mix(hash21(i + vec2(0.0, 1.0)), hash21(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm(vec2 p) {
    mat2 rotate = mat2(0.8776, 0.4794, -0.4794, 0.8776);
    float value = 0.0;
    float amplitude = 0.5;
    for (int i = 0; i < 3; ++i) {
        value += amplitude * noise(p);
        p = rotate * (p * 2.03) + vec2(13.7, 5.3);
        amplitude *= 0.5;
    }
    return value * 1.142857;
}

vec3 mood(float height) {
    float y = clamp(height, 0.0, 1.0);
    return y < 0.5
        ? mix(paletteTop.rgb, paletteMid.rgb, y * 2.0)
        : mix(paletteMid.rgb, paletteBottom.rgb, (y - 0.5) * 2.0);
}

float boundary(float angle, float radius, float phase, float motion, float spin) {
    float a = angle + time * spin;
    float breathe = 1.0 + 0.22 * sin(time * 0.40 + phase);
    float idle = 0.045 * sin(a * 3.0 + time * 0.70 + phase)
               + 0.028 * sin(a * 5.0 - time * 0.50 + phase * 1.7)
               + 0.018 * sin(a * 2.0 + time * 0.26 + phase * 0.5);
    float react = (0.060 * sin(a * 3.0 + time * 1.15 + phase)
                 + 0.042 * sin(a * 6.0 - time * 0.95 + phase * 1.3)
                 + 0.026 * sin(a * 9.0 + time * 0.80)
                 + 0.015 * sin(a * 13.0 - time * 0.62 + phase * 2.1)) * motion;
    return radius + (idle + react) * breathe;
}

float blobRadius(float angle) {
    return 0.118 + 0.010 * sin(3.0 * angle + time * 0.70)
        + 0.009 * sin(4.0 * angle - time * 0.60 + 1.2)
        + 0.006 * sin(2.0 * angle + time * 0.40)
        + 0.005 * sin(6.0 * angle - time * 0.30 + 2.0);
}

void main() {
    vec2 res = max(resolution, vec2(1.0));
    // ShaderEffect guarantees qt_TexCoord0 even when custom uniforms are not
    // initialized during the first scenegraph frame.
    vec2 pc = (qt_TexCoord0 - 0.5) * vec2(res.x / res.y, 1.0);
    vec2 drift = 0.008 * vec2(sin(time * 0.16 + 1.1), cos(time * 0.13));
    vec2 p = pc - drift;
    float bass = clamp(audio.x, 0.0, 1.0);
    float mid = clamp(audio.y, 0.0, 1.0);
    float high = clamp(audio.z, 0.0, 1.0);
    float beat = clamp(audio.w, 0.0, 1.0);
    float radius = length(pc);

    float swirl = (0.05 + mid * 0.035) * sin(time * 0.18 + radius * 3.0) * (0.4 + radius);
    mat2 rotation = mat2(cos(swirl), -sin(swirl), sin(swirl), cos(swirl));
    vec2 q = rotation * p;
    float qr = length(q);
    float angle = atan(q.y, q.x) + PI;
    float motion = clamp(mid * 0.70 + bass * 0.75 + high * 0.35 + beat * 0.55, 0.0, 1.25);

    float leaf0 = boundary(angle, 0.365, 0.0, motion, 0.420);
    float leaf1 = boundary(angle, 0.275, 1.5, motion, -0.280);
    float leaf2 = boundary(angle, 0.195, 2.8, motion, 0.180);
    float aa = max(fwidth(qr), 1.0 / res.y);
    float m0 = 1.0 - smoothstep(leaf0 - aa, leaf0 + aa, qr);
    float m1 = 1.0 - smoothstep(leaf1 - aa, leaf1 + aa, qr);
    float m2 = 1.0 - smoothstep(leaf2 - aa, leaf2 + aa, qr);

    float vertical = clamp(qt_TexCoord0.y, 0.0, 1.0);
    vec3 col = mix(vec3(0.042, 0.040, 0.068), mood(vertical) * 0.22, 0.62);
    float nebula = fbm(q * 1.5 + vec2(time * 0.03, -time * 0.02));
    col += mix(paletteTop.rgb, paletteGlow.rgb, 0.5) * 0.075 * nebula * (0.6 + 0.5 * bass + 0.2 * mid);
    col = mix(col, mood(0.20) * (0.52 + mid * 0.24), m0);
    col = mix(col, mood(0.50) * (0.60 + mid * 0.20), m1);
    col = mix(col, mood(0.82) * (0.70 + high * 0.16), m2);

    col += paletteGlow.rgb * 0.38 * exp(-abs(qr - leaf0) * (150.0 + high * 30.0)) * (1.0 + bass * 0.30);
    col += paletteGlow.rgb * 0.44 * exp(-abs(qr - leaf1) * 160.0) * (1.0 + mid * 0.30);
    col += paletteGlow.rgb * 0.52 * exp(-abs(qr - leaf2) * 170.0) * (1.0 + high * 0.30);

    vec2 blobPosition = p - vec2(0.010 * sin(time * 0.35), 0.007 * cos(time * 0.28));
    float blobDistance = length(blobPosition);
    float blobRadiusValue = blobRadius(atan(blobPosition.y, blobPosition.x) + time * 0.22) * (1.0 + 0.012 * sin(time * 0.9));
    float blobMask = 1.0 - smoothstep(blobRadiusValue - aa, blobRadiusValue + aa, blobDistance);
    float shade = clamp(0.5 + blobPosition.y * 1.6, 0.0, 1.0);
    vec3 blobColor = mix(paletteTop.rgb, paletteGlow.rgb, shade);
    blobColor = mix(blobColor, paletteBottom.rgb, clamp(0.5 + blobPosition.x * 1.4, 0.0, 1.0) * 0.45);
    blobColor *= 0.90 * (1.0 + high * 0.12);
    col = mix(col, blobColor, blobMask);
    col += mix(vec3(1.0), paletteGlow.rgb, 0.35) * exp(-abs(blobDistance - blobRadiusValue) * (18.0 + high * 10.0)) * blobMask * 0.18;
    col += paletteGlow.rgb * exp(-max(blobDistance - blobRadiusValue, 0.0) * 45.0) * 0.16 * (1.0 + bass * 0.8);

    col *= 0.82 + 0.18 * (1.0 - smoothstep(0.45, 1.15, radius));
    float alpha = 0.74 * qt_Opacity;
    fragColor = vec4(col * alpha, alpha);
}
