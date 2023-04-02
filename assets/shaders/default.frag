#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"

//shader input
layout (location = 0) in vec3 inColor;
layout (location = 1) in vec2 inTexCoords;
layout (location = 2) in vec3 inNormal;

layout (location = 0) out vec4 outFragColor;

struct Light{
	vec4 position;
	vec4 colour;
};

layout(std140,set = 1, binding = 1) uniform LightBuffer{
	Light lights[4];
} lightData;

layout( push_constant ) uniform constants
{
	mat4 model;
	ivec4 textures;
} pushConstants;

void main()
{
	vec3 outColour = inColor;
	vec4 diffuseTexture = SampleBindlessTexture(pushConstants.textures.r, inTexCoords);
	if (diffuseTexture.a == 0){
		discard;
	}
	outColour *= diffuseTexture.rgb;

	vec4 normalTexture = SampleBindlessTexture(pushConstants.textures.g, inTexCoords);

	float metallicFactor = SampleBindlessTexture(pushConstants.textures.b, inTexCoords).b;
	float roughnessFactor = SampleBindlessTexture(pushConstants.textures.b, inTexCoords).g;

	vec4 emissiveTexture = SampleBindlessTexture(pushConstants.textures.a, inTexCoords);
	outColour += emissiveTexture.rgb;

	outFragColor = vec4(outColour,1.0f);
}