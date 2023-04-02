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
	float ambientFactor = SampleBindlessTexture(pushConstants.textures.a, inTexCoords).r;

	//vec4 emissiveTexture = SampleBindlessTexture(pushConstants.textures.a, inTexCoords);
	//outColour += emissiveTexture.rgb;

	vec3 albedo     = pow(diffuseTexture.rgb, vec3(2.2));
	vec3 normal     = normalTexture.rgb;
	float metallic  = metallicFactor;
	float roughness = roughnessFactor;
	float ao        = ambientFactor;

	vec3 N = normalize(inNormal);
	vec3 V = normalize(cameraData.cameraPos.xyz - inWorldPos);

	vec3 F0 = vec3(0.04);
	F0 = mix(F0, albedo, metallic);

	// reflectance equation
	vec3 Lo = vec3(0.0);
	for(int i = 0; i < 4; ++i)
	{
		// calculate per-light radiance
		vec3 L = normalize(lightData.lights[i].position.xyz - inWorldPos);
		vec3 H = normalize(V + L);
		float distance    = length(lightData.lights[i].position.xyz - inWorldPos);
		float attenuation = 1.0 / (distance * distance);
		vec3 radiance     = lightData.lights[i].colour.rgb * attenuation;

		// cook-torrance brdf
		float NDF = DistributionGGX(N, H, roughness);
		float G   = GeometrySmith(N, V, L, roughness);
		vec3 F    = fresnelSchlick(max(dot(H, V), 0.0), F0);

		vec3 kS = F;
		vec3 kD = vec3(1.0) - kS;
		kD *= 1.0 - metallic;

		vec3 numerator    = NDF * G * F;
		float denominator = 4.0 * max(dot(N, V), 0.0) * max(dot(N, L), 0.0) + 0.0001;
		vec3 specular     = numerator / denominator;

		// add to outgoing radiance Lo
		float NdotL = max(dot(N, L), 0.0);
		Lo += (kD * albedo / PI + specular) * radiance * NdotL;
	}

	vec3 ambient = vec3(0.03) * albedo * ao;
	vec3 color = ambient + Lo;

	color = color / (color + vec3(1.0));
	color = pow(color, vec3(1.0/2.2));

	outFragColor = vec4(color,1.0f);
}