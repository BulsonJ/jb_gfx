#version 460
#include "assets/shaders/library/texture.glsl"

layout (location = 0) in vec4 inColor;
layout (location = 1) in vec2 inTexCoords;
layout (location = 2) in flat int inTexHandle;

layout (location = 0) out vec4 outFragColor;

void main()
{
    if (inTexHandle > 0){
        vec4 texture = SampleBindlessTexture(2, inTexHandle, inTexCoords);
        outFragColor = inColor * texture;
    } else {
        outFragColor = vec4(inColor);
    }
}