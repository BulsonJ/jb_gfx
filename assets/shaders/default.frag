#version 460
#include "assets/shaders/library/pbr.glsl"
#include "assets/shaders/library/texture.glsl"

//shader input
layout (location = 0) in vec3 inColor;
layout (location = 1) in vec2 inTexCoords;
layout (location = 2) in vec3 inNormal;
layout (location = 3) in vec3 inWorldPos;
layout (location = 4) in mat3 inTBN;

layout (location = 0) out vec4 outFragColor;

struct Light{
	vec4 position;
	vec4 colour;
};

layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
	mat4 proj;
	mat4 view;
	vec4 cameraPos;
	vec4 ambientLight;
	vec4 directionalLightColour;
	vec4 directionalLightDirection;
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

	vec4 diffuseTexture = SampleBindlessTexture(diffuseTexIndex, inTexCoords);
	vec3 emissiveTexture = SampleBindlessTexture(emissiveTexIndex, inTexCoords).rgb;

	// Ambient
	vec3 objectColour = inColor;
	if (diffuseTexIndex > 0) {
		if (diffuseTexture.a == 0){
			discard;
		}
		objectColour *= diffuseTexture.rgb;
	} else {
		objectColour *= material.diffuse.rgb;
	}
	vec3 ambient = cameraData.ambientLight.w * cameraData.ambientLight.rgb;

	vec3 normal = normalize(inNormal);
	if (normalTexIndex > 0){
		vec3 normalTexture = SampleBindlessTexture(normalTexIndex, inTexCoords).rgb;
		normal = normalize(inTBN * normalize(normalTexture * 2.0 - 1.0));
	}

	vec3 diffuseResult = vec3(0);
	vec3 specularResult = vec3(0);
	// Scene directional light
	{
		vec3 diffuse = vec3(0);
		vec3 specular = vec3(0);

		vec3 lightDir = cameraData.directionalLightDirection.xyz;
		float diff = max(dot(normal, lightDir), 0.0);
		diffuse += diff * cameraData.directionalLightColour.rgb;

		// Specular
		float shininess = 32.0;
		float specularStrength = 0.2;
		vec3 viewDir = normalize(cameraData.cameraPos.xyz - inWorldPos);
		vec3 halfwayDir = normalize(lightDir + viewDir);
		float spec = pow(max(dot(normal, halfwayDir), 0.0), shininess);
		specular += specularStrength * spec * cameraData.directionalLightColour.rgb;

		diffuseResult += diffuse;
		specularResult += specular;
	}

	// Point lights

	for (int i = 0; i < 4; i++){
		// Diffuse
		Light currentLight = lightData.lights[i];
		vec3 diffuse = vec3(0);
		vec3 specular = vec3(0);

		vec3 lightDir = normalize(currentLight.position.xyz - inWorldPos);
		float diff = max(dot(normal, lightDir), 0.0);
		diffuse += diff * currentLight.colour.rgb;

		// Specular
		float shininess = 32.0;
		float specularStrength = 0.2;
		vec3 viewDir = normalize(cameraData.cameraPos.xyz - inWorldPos);
		vec3 halfwayDir = normalize(lightDir + viewDir);
		float spec = pow(max(dot(normal, halfwayDir), 0.0), shininess);
		specular += specularStrength * spec * currentLight.colour.rgb;

		// attenuation
		float distance    = length(currentLight.position.xyz - inWorldPos);
		float lightConstant = 1.0;
		float lightLinear = 0.09;
		float lightQuadratic = 0.032;
		float attenuation = 1.0 / (lightConstant + lightLinear * distance + lightQuadratic * (distance * distance));

		diffuse *= attenuation;
		specular *= attenuation;

		diffuseResult += diffuse;
		specularResult += specular;
	}

	vec3 result = (ambient + diffuseResult + specularResult) * objectColour;

	// Emissive
	if (emissiveTexIndex > 0){
		result += emissiveTexture;
	} else {
		result += material.emissive.rgb;
	}

	outFragColor = vec4(result,1.0f);
}