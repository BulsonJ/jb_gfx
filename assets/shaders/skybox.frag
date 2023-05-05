#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"
#include "assets/shaders/library/shadow.glsl"
#include "assets/shaders/library/lighting.glsl"

//shader input
layout (location = 0) in vec3 inViewDir;

layout (location = 0) out vec4 gPosition;
layout (location = 1) out vec4 gNormal;
layout (location = 2) out vec4 gAlbedoSpec;

layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
    mat4 proj;
    mat4 view;
    mat4 invProjView;
    vec4 cameraPos;
    vec4 ambientLight;
    vec3 directionalLightColour;
    float directionalLightStrength;
    vec4 directionalLightDirection;
    mat4 sunProj;
    mat4 sunView;
    int pointLightCount;
    int padding[3];
} cameraData;

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