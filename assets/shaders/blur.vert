//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable

layout (location = 0) out vec2 outTexCoords;

layout (set = 0, binding = 0) uniform sampler2D bloomImage;

void main()
{
	const vec2 positions[] = vec2[](
	vec2(-1.f,-1.f),
	vec2(1.f,0.f),
	vec2(1.f,1.f),
	vec2(-1.f,-1.f),
	vec2(1.f,1.f),
	vec2(-1.f,1.f)
	);

	const vec2 texCoords[] = vec2[](
	vec2(0.f,0.f),
	vec2(1.f,0.f),
	vec2(1.f,1.f),
	vec2(0.f,0.f),
	vec2(1.f,1.f),
	vec2(0.f,1.f)
	);

	int transformIndex = gl_VertexIndex / 6;
	int vertexIndex = gl_VertexIndex % 6;

	outTexCoords = texCoords[vertexIndex];
	gl_Position = vec4(positions[vertexIndex], 0.0f, 1.0f);
}