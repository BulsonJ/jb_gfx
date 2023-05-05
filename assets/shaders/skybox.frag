#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"
#include "assets/shaders/library/shadow.glsl"
#include "assets/shaders/library/lighting.glsl"
#include "assets/shaders/library/camera.glsl"

//shader input
layout (location = 0) in vec3 inViewDir;

layout (location = 0) out vec4 gPosition;
layout (location = 1) out vec4 gNormal;
layout (location = 2) out vec4 gAlbedoSpec;

layout( push_constant ) uniform constants
{
    int handle;
} pushConstants;

void main()
{
    vec3 skybox = SampleBindlessSkybox(3, pushConstants.handle, inViewDir).rgb;
    //skybox = inViewDir;
    gAlbedoSpec.rgb = skybox;
}