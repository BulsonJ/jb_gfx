#version 460
#include "assets/shaders/library/texture.glsl"

layout (location = 0) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;

layout( push_constant ) uniform constants
{
    ivec4 handles;
} pushConstants;

void main()
{
    vec3 colour = vec3(1,1,1);
    int textureHandle = pushConstants.handles.g;
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