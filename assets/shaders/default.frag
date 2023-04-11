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
	vec4 ambientLight;
} cameraData;

layout(std140,set = 1, binding = 1) uniform LightBuffer{
	Light lights[4];
} lightData;

struct MaterialParameters {
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
	vec4 diffuseTexture = SampleBindlessTexture(material.textures.r, inTexCoords);
	if (diffuseTexture.a == 0){
		discard;
	}
	vec3 normalTexture = SampleBindlessTexture(material.textures.g, inTexCoords).rgb;

	float metallicFactor = SampleBindlessTexture(material.textures.b, inTexCoords).b;
	float roughnessFactor = SampleBindlessTexture(material.textures.b, inTexCoords).g;
	float ambientFactor = SampleBindlessTexture(material.textures.a, inTexCoords).r;

	vec3 emissiveTexture = SampleBindlessTexture(material.textures_two.r, inTexCoords).rgb;

	// Ambient
	vec3 objectColour = inColor * diffuseTexture.rgb;
	vec3 ambient = cameraData.ambientLight.w * cameraData.ambientLight.rgb;

	// Point lights
	vec3 diffuse = vec3(0);
	vec3 specular = vec3(0);
	for (int i = 0; i < 4; i++){
		Light currentLight = lightData.lights[i];
		vec3 norm = normalize(inNormal);
		vec3 lightDir = normalize(currentLight.position.xyz - inWorldPos);

		float diff = max(dot(norm, lightDir), 0.0);
		diffuse += diff * currentLight.colour.rgb;

		// Specular
		float specularStrength = 0.5;
		vec3 viewDir = normalize(cameraData.cameraPos.xyz - inWorldPos);
		vec3 reflectDir = reflect(-lightDir, norm);

		float spec = pow(max(dot(viewDir, reflectDir), 0.0), 32);
		specular += specularStrength * spec * currentLight.colour.rgb;
	}

	vec3 result = (ambient + diffuse + specular) * objectColour;
	outFragColor = vec4(result,1.0f);
}