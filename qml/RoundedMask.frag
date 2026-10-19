#version 440

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;
    float radius;
};

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(binding = 1) uniform sampler2D source;

void main() {
    vec2 halfSize = size * 0.5;
    vec2 q = abs(qt_TexCoord0 * size - halfSize) - (halfSize - radius);
    float dist = length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    float alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
    fragColor = texture(source, qt_TexCoord0) * alpha * qt_Opacity;
}
