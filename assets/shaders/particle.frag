#version 460
#include "assets/shaders/library/texture.glsl"
#include "assets/shaders/library/shadow.glsl"
#include "assets/shaders/library/lighting.glsl"
#include "assets/shaders/library/camera.glsl"

layout (location = 0) in vec2 inTexCoords;
layout (location = 1) flat in int inParticleInstance;

layout (location = 0) out vec4 outFragColor;
layout (location = 1) out vec4 outBrightColor;

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
	Particle self = particleData.particles[inParticleInstance];

	vec4 colour = self.colour;
	if (self.textureIndex > 0) {
		colour *= SampleBindlessTexture(0, self.textureIndex, inTexCoords);
	}

	outFragColor = colour;

	// Bright Colours
	float brightness = dot(outFragColor.rgb, vec3(0.2126, 0.7152, 0.0722));
	if(brightness > 1.0) {
		outBrightColor = vec4(outFragColor.rgb, 1.0);
	}
	else {
		outBrightColor = vec4(0.0, 0.0, 0.0, 1.0);
	}
}