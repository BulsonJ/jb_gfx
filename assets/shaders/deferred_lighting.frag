#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"
#include "assets/shaders/library/shadow.glsl"
#include "assets/shaders/library/lighting.glsl"

layout (location = 0) in vec2 inTexCoords;

layout (location = 0) out vec4 outFragColor;
layout (location = 1) out vec4 outBrightColor;

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

layout (set = 1, binding = 4) uniform sampler2DShadow sceneShadowMap;

layout (set = 2, binding = 0) uniform sampler2D positionImage;
layout (set = 2, binding = 1) uniform sampler2D normalImage;
layout (set = 2, binding = 2) uniform sampler2D albedoSpecImage;
layout (set = 2, binding = 3) uniform sampler2D depthImage;

const mat4 biasMat = mat4(
0.5, 0.0, 0.0, 0.0,
0.0, 0.5, 0.0, 0.0,
0.0, 0.0, 1.0, 0.0,
0.5, 0.5, 0.0, 1.0 );

void main()
{
    float depth = texture(depthImage, inTexCoords).r;
    vec4 ndc = vec4(vec2(inTexCoords.x,inTexCoords.y) * 2.0 -1.0,depth, 1.0f);
    vec4 clip = cameraData.invProjView * ndc;
    vec3 fragPos = clip.xyz / clip.www;

    vec3 emissive = texture(positionImage, inTexCoords).rgb;
    vec3 normal = texture(normalImage, inTexCoords).rgb;
    vec3 albedo = texture(albedoSpecImage, inTexCoords).rgb;
    float specular = texture(albedoSpecImage, inTexCoords).a;

    vec3 ambient = cameraData.ambientLight.w * cameraData.ambientLight.rgb;

    // calculate shadow
    vec4 inShadowCoord = biasMat * cameraData.sunProj * cameraData.sunView * vec4(fragPos, 1.0f);
    float shadow = ShadowCalculation(sceneShadowMap, inShadowCoord / inShadowCoord.w);

    // ----------------- Lighting Calculations -----------------------
    // Directional Light
    vec3 dirLight = CalculateDirectionalLight(normal, fragPos,cameraData.cameraPos.xyz, -cameraData.directionalLightDirection.xyz,cameraData.directionalLightColour,cameraData.directionalLightStrength);
    vec3 lighting = (1.0 - shadow) * (dirLight);

    // Point lights
    vec3 pointLightsResult = vec3(0);
    for (int i = 0; i < cameraData.pointLightCount; i++){
        // Diffuse
        Light currentLight = lightData.lights[i];
        pointLightsResult += CalculatePointLight(normal, fragPos,cameraData.cameraPos.xyz, currentLight);
    }
    lighting += pointLightsResult;
    vec3 result = albedo * (ambient + lighting);
    // ----------------- Lighting Calculations -----------------------

    result += emissive;

    if (depth == 1){
        result = albedo;
    }

    outFragColor = vec4(result,1.0f);

    // Bright Colours
    float brightness = dot(outFragColor.rgb, vec3(0.2126, 0.7152, 0.0722));
    if(brightness > 1.0) {
        outBrightColor = vec4(outFragColor.rgb, 1.0);
    }
    else {
        outBrightColor = vec4(0.0, 0.0, 0.0, 1.0);
    }
}