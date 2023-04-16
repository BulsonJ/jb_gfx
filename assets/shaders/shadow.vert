//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec2 vTexCoords;
layout (location = 2) in vec3 vNormal;
layout (location = 3) in vec3 vColor;
layout (location = 4) in vec4 vTangent;

layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
	mat4 proj;
	mat4 view;
	vec4 cameraPos;
	vec4 ambientLight;
	vec4 directionalLightColour;
	vec4 directionalLightDirection;
} cameraData;

struct ModelMatrix{
	mat4 model;
	mat4 normal;
};

layout(std140,set = 1, binding = 2) readonly buffer ModelBuffer{
	ModelMatrix models[];
} modelData;

layout( push_constant ) uniform constants
{
	ivec4 handles;
} pushConstants;

void main()
{
	mat4 modelMatrix = modelData.models[pushConstants.handles.x].model;
	gl_Position = cameraData.proj * cameraData.view * modelMatrix * vec4(vPosition, 1.0f);
}