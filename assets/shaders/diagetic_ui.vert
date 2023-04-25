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

struct DiageticUIDrawData{
	vec3 position;
	int textureIndex;
	vec3 colour;
	float size;
};

layout(std140,set = 1, binding = 1) readonly buffer DrawDataBuffer{
	DiageticUIDrawData draw[];
} drawData;

layout( push_constant ) uniform constants
{
	int handle;
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

	vec3 camera_right_world = vec3(cameraData.view[0][0], cameraData.view[1][0], cameraData.view[2][0]);
	vec3 camera_up_world = vec3(cameraData.view[0][1], cameraData.view[1][1], cameraData.view[2][1]);

	vec2 billboard_size = vec2(drawData.draw[pushConstants.handle].size);
	vec3 position = drawData.draw[pushConstants.handle].position;

	vec3 vertex_pos_world = position
		+ (camera_right_world * positions[vertexIndex].x * billboard_size.x)
		+ (camera_up_world * positions[vertexIndex].y * billboard_size.y);

	gl_Position = cameraData.proj * cameraData.view * vec4(vertex_pos_world, 1.0f);
}