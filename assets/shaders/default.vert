//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) out vec3 outColor;

layout(std140,set = 0, binding = 0) uniform  CameraBuffer{
	mat4 proj;
	mat4 view;
} cameraData;

layout( push_constant ) uniform constants
{
	mat4 model;
} pushConstants;

void main()
{
	//const array of positions for the triangle
	const vec3 positions[3] = vec3[3](
		vec3(1.f,1.f, 0.0f),
		vec3(-1.f,1.f, 0.0f),
		vec3(0.f,-1.f, 0.0f)
	);

	//const array of colors for the triangle
	const vec3 colors[3] = vec3[3](
		vec3(1.0f, 0.0f, 0.0f), //red
		vec3(0.0f, 1.0f, 0.0f), //green
		vec3(00.f, 0.0f, 1.0f)  //blue
	);
	//output the position of each vertex
	gl_Position = cameraData.proj * cameraData.view * pushConstants.model * vec4(positions[gl_VertexIndex], 1.0f);
	outColor = colors[gl_VertexIndex];
}