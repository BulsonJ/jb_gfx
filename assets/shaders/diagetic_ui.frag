#version 460
#include "assets/shaders/library/texture.glsl"

layout (location = 0) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;

struct DiageticUIDrawData{
    vec3 position;
    int textureIndex;
    vec3 colour;
    float size;
};

layout(std140,set = 1, binding = 1) readonly buffer DrawDataBuffer{
    DiageticUIDrawData draw[];
} drawData;

layout( push_constant ) uniform constants
{
    int handle;
} pushConstants;

void main()
{
    vec3 colour = drawData.draw[pushConstants.handle].colour;
    int textureHandle = drawData.draw[pushConstants.handle].textureIndex;
    if (textureHandle > 0){
        vec4 texture = SampleBindlessTexture(2, textureHandle, inTexCoords);
        if (texture.a == 0){
            discard;
        }
        outFragColor = vec4(colour, 1.0f);
    } else {
        outFragColor = vec4(0.0f);
    }
}