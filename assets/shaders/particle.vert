//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable
#include "assets/shaders/library/camera.glsl"

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec2 vTexCoords;
layout (location = 2) in vec3 vNormal;
layout (location = 3) in vec3 vColor;
layout (location = 4) in vec4 vTangent;

layout (location = 0) out vec2 outTexCoords;
layout (location = 1) out int outParticleInstance;

struct Particle{
	mat4 model;
	vec4 colour;
	int textureIndex;
	float padding;
	float padding_two;
	float padding_three;
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

	outParticleInstance = gl_InstanceIndex;
	Particle self = particleData.particles[gl_InstanceIndex];

	outTexCoords = vTexCoords;

	gl_Position = cameraData.proj * cameraData.view * self.model * vec4(vPosition, 1.0f);
}