#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) out vec2 outTexCoords;

layout(std140,set = 1, binding = 0) uniform  CameraBuffer{
	mat4 proj;
	mat4 view;
	vec4 cameraPos;
	vec4 ambientLight;
	vec4 directionalLightColour;
	vec4 directionalLightDirection;
	mat4 sunProj;
	mat4 sunView;
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
	const vec2 positions[] = vec2[](
	vec2(-1.f,-1.f),
	vec2(1.f,-1.f),
	vec2(1.f,1.f),
	vec2(-1.f,-1.f),
	vec2(1.f,1.f),
	vec2(-1.f,1.f)
	);

	const vec2 texCoords[] = vec2[](
	vec2(1.f,1.f),
	vec2(0.f,1.f),
	vec2(0.f,0.f),
	vec2(1.f,1.f),
	vec2(0.f,0.f),
	vec2(1.f,0.f)
	);

	int vertexIndex = gl_VertexIndex % 6;
	outTexCoords = texCoords[vertexIndex];

	vec3 CameraRight_worldspace = vec3(cameraData.view[0][0], cameraData.view[1][0], cameraData.view[2][0]);
	vec3 CameraUp_worldspace = vec3(cameraData.view[0][1], cameraData.view[1][1], cameraData.view[2][1]);

	mat4 modelMatrix = modelData.models[pushConstants.handles.x].model;

	vec2 BillboardSize = vec2(2.5,2.5);
	vec3 position = vec3(0,10,0);

	vec3 vertexPosition_worldspace = //position +
		CameraRight_worldspace * positions[vertexIndex].x * BillboardSize.x
		+ CameraUp_worldspace * positions[vertexIndex].y * BillboardSize.y;

	gl_Position = cameraData.proj * cameraData.view * modelMatrix * vec4(vertexPosition_worldspace, 1.0f);
}