#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"
#include "assets/shaders/library/shadow.glsl"
#include "assets/shaders/library/lighting.glsl"

//shader input
layout (location = 0) in vec3 inColor;
layout (location = 1) in vec2 inTexCoords;
layout (location = 2) in vec3 inNormal;
layout (location = 3) in vec3 inWorldPos;
layout (location = 4) in mat3 inTBN;
layout (location = 7) in vec4 inShadowCoord;

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

layout(std140,set = 1, binding = 1) uniform LightBuffer{
    Light lights[4];
} lightData;

struct MaterialParameters {
    vec4 diffuse;
    vec4 emissive;
    ivec4 textures;
    ivec4 textures_two;
};

layout(std140,set = 1, binding = 3) readonly buffer MaterialBuffer{
    MaterialParameters materials[];
} materialData;

layout (set = 1, binding = 4) uniform sampler2DShadow sceneShadowMap;

layout( push_constant ) uniform constants
{
    ivec4 handles;
} pushConstants;

void main()
{
    MaterialParameters material = materialData.materials[pushConstants.handles.g];
    int diffuseTexIndex = material.textures.r;
    int normalTexIndex = material.textures.g;
    int emissiveTexIndex = material.textures_two.r;

    vec4 diffuseTexture = SampleBindlessTexture(0, diffuseTexIndex, inTexCoords);
    vec3 emissiveTexture = SampleBindlessTexture(0, emissiveTexIndex, inTexCoords).rgb;

    // Ambient
    vec3 objectColour = inColor;
    if (diffuseTexIndex > 0) {
        if (diffuseTexture.a == 0){
            discard;
        }
        objectColour *= diffuseTexture.rgb * material.diffuse.rgb;
    } else {
        objectColour *= material.diffuse.rgb;
    }

    vec3 normal = normalize(inNormal);
    if (normalTexIndex > 0){
        vec3 normalTexture = SampleBindlessTexture(0, normalTexIndex, inTexCoords).rgb;
        normal = normalize(inTBN * normalize(normalTexture * 2.0 - 1.0));
    }

    vec3 emissive = material.emissive.rgb;
    if (emissiveTexIndex > 0) {
        emissive *= emissiveTexture.rgb * emissive;
    }

    gPosition = vec4(emissive, 1.0f);
    gNormal = vec4(normal, 1.0f);
    gAlbedoSpec.rgb = objectColour;
    gAlbedoSpec.a = 1.0;
}