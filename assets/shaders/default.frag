#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"

//shader input
layout (location = 0) in vec3 inColor;
layout (location = 1) in vec2 inTexCoords;
layout (location = 2) in vec3 inNormal;
layout (location = 3) in vec3 inWorldPos;

layout (location = 0) out vec4 outFragColor;

struct Light{
	vec4 position;
	vec4 colour;
};

layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
	mat4 proj;
	mat4 view;
	vec4 cameraPos;
} cameraData;

layout(std140,set = 1, binding = 1) uniform LightBuffer{
	Light lights[4];
} lightData;

layout( push_constant ) uniform constants
{
	ivec4 handles;
	ivec4 textures;
	ivec4 textures_two;
} pushConstants;

vec3 getNormalFromMap()
{
	vec3 tangentNormal = SampleBindlessTexture(pushConstants.textures.g, inTexCoords).xyz * 2.0 - 1.0;

	vec3 Q1  = dFdx(inWorldPos);
	vec3 Q2  = dFdy(inWorldPos);
	vec2 st1 = dFdx(inTexCoords);
	vec2 st2 = dFdy(inTexCoords);

	vec3 N   = normalize(inNormal);
	vec3 T  = normalize(Q1*st2.t - Q2*st1.t);
	vec3 B  = -normalize(cross(N, T));
	mat3 TBN = mat3(T, B, N);

	return normalize(TBN * tangentNormal);
}

void main()
{
	vec4 diffuseTexture = SampleBindlessTexture(pushConstants.textures.r, inTexCoords);
	if (diffuseTexture.a == 0){
		discard;
	}
	vec3 normalTexture = SampleBindlessTexture(pushConstants.textures.g, inTexCoords).rgb;

	float metallicFactor = SampleBindlessTexture(pushConstants.textures.b, inTexCoords).b;
	float roughnessFactor = SampleBindlessTexture(pushConstants.textures.b, inTexCoords).g;
	float ambientFactor = SampleBindlessTexture(pushConstants.textures.a, inTexCoords).r;

	vec3 emissiveTexture = SampleBindlessTexture(pushConstants.textures_two.r, inTexCoords).rgb;

	vec3 outColour = inColor;
	outColour = diffuseTexture.rgb;
	outColour += emissiveTexture;

	outFragColor = vec4(outColour,1.0f);
}