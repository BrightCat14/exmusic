#version 440

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 texelSize;
    float radius;
};

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(binding = 1) uniform sampler2D source;

const int KERNEL_RADIUS = 4;
const float WEIGHTS[5] = float[](
    0.2270270270,
    0.1945945946,
    0.1216216216,
    0.0540540541,
    0.0162162162
);

void main() {
    vec2 step = texelSize * (radius / float(KERNEL_RADIUS));
    vec4 color = vec4(0.0);
    for (int y = -KERNEL_RADIUS; y <= KERNEL_RADIUS; ++y) {
        for (int x = -KERNEL_RADIUS; x <= KERNEL_RADIUS; ++x) {
            float weight = WEIGHTS[abs(x)] * WEIGHTS[abs(y)];
            color += texture(source, qt_TexCoord0 + vec2(float(x), float(y)) * step) * weight;
        }
    }
    fragColor = color * qt_Opacity;
}
