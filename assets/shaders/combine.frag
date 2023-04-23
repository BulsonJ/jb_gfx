#version 460

layout (location = 0) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;

layout (set = 0, binding = 0) uniform sampler2D forwardImage;
layout (set = 0, binding = 1) uniform sampler2D bloomImage;

void main()
{
    vec3 forwardColour = texture(forwardImage, inTexCoords).rgb;
    vec3 bloomColour = texture(bloomImage, inTexCoords).rgb;
    vec3 combineResult = forwardColour + bloomColour;

    outFragColor = vec4(combineResult,1.0f);
}