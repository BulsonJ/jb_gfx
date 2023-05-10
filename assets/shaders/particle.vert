//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable
#include "assets/shaders/library/camera.glsl"

layout (location = 0) out vec2 outTexCoords;
layout (location = 1) out int outParticleInstance;

struct Particle{
	vec3 position;
	int textureIndex;
	vec3 colour;
	float size;
};

layout(std140,set = 2, binding = 0) readonly buffer ParticleBuffer{
	Particle particles[];
} particleData;

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
	vec2 pos = positions[vertexIndex];
	outTexCoords = texCoords[vertexIndex];

	outParticleInstance = gl_InstanceIndex;
	Particle self = particleData.particles[gl_InstanceIndex];

	vec3 camera_right_world = vec3(cameraData.view[0][0], cameraData.view[1][0], cameraData.view[2][0]);
	vec3 camera_up_world = vec3(cameraData.view[0][1], cameraData.view[1][1], cameraData.view[2][1]);

	vec2 billboard_size = vec2(self.size);
	vec3 position = self.position;

	vec3 vertex_pos_world = position
	+ (camera_right_world * positions[vertexIndex].x * billboard_size.x)
	+ (camera_up_world * positions[vertexIndex].y * billboard_size.y);

	gl_Position = cameraData.proj * cameraData.view * vec4(vertex_pos_world, 1.0f);
}