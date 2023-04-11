//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec2 vTexCoords;
layout (location = 2) in vec3 vNormal;
layout (location = 3) in vec3 vColor;

layout (location = 0) out vec3 outColor;
layout (location = 1) out vec2 outTexCoords;
layout (location = 2) out vec3 outNormal;
layout (location = 3) out vec3 outWorldPos;

layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
	mat4 proj;
	mat4 view;
	vec4 cameraPos;
	vec4 ambientLight;
} cameraData;

struct ModelMatrix{
	mat4 model;
	mat4 normal;
};

struct MaterialParameters {
	ivec4 textures;
	ivec4 textures_two;
};

layout(std140,set = 1, binding = 2) readonly buffer ModelBuffer{
	ModelMatrix models[];
} modelData;

layout(std140,set = 1, binding = 3) readonly buffer MaterialBuffer{
	MaterialParameters materials[];
} materialData;

layout( push_constant ) uniform constants
{
	ivec4 handles;
} pushConstants;

void main()
{
	mat4 modelMatrix = modelData.models[pushConstants.handles.x].model;
	mat4 normalMatrix = modelData.models[pushConstants.handles.x].normal;
	vec3 worldPos = vec3(modelMatrix * vec4(vPosition, 1.0f));
	outWorldPos = worldPos;
	outColor = vColor;
	outTexCoords = vTexCoords;
	outNormal = mat3(normalMatrix) * vNormal;
	gl_Position = cameraData.proj * cameraData.view * modelMatrix * vec4(vPosition, 1.0f);
}