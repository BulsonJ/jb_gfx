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
	vec3 ambient = cameraData.ambientLight.w * cameraData.ambientLight.rgb;

	vec3 normal = normalize(inNormal);
	if (normalTexIndex > 0){
		vec3 normalTexture = SampleBindlessTexture(0, normalTexIndex, inTexCoords).rgb;
		normal = normalize(inTBN * normalize(normalTexture * 2.0 - 1.0));
	}

	// calculate shadow
	float shadow = ShadowCalculation(sceneShadowMap, inShadowCoord / inShadowCoord.w);

	// ----------------- Lighting Calculations -----------------------
	// Directional Light
	vec3 dirLight = CalculateDirectionalLight(normal, inWorldPos,cameraData.cameraPos.xyz, -cameraData.directionalLightDirection.xyz,cameraData.directionalLightColour,cameraData.directionalLightStrength);
	vec3 lighting = (1.0 - shadow) * (dirLight);

	// Point lights
	vec3 pointLightsResult = vec3(0);
	for (int i = 0; i < 4; i++){
		// Diffuse
		Light currentLight = lightData.lights[i];
		pointLightsResult += CalculatePointLight(normal, inWorldPos,cameraData.cameraPos.xyz, currentLight);
	}
	lighting += pointLightsResult;
	vec3 result = objectColour * (ambient + lighting);
	// ----------------- Lighting Calculations -----------------------

	// Emissive
	if (emissiveTexIndex > 0){
		result += emissiveTexture * material.emissive.rgb;
	} else {
		result += material.emissive.rgb;
	}

	// Normal Fragment Colour
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