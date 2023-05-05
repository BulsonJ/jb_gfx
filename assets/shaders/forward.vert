//we will be using glsl version 4.5 syntax
#version 450
#extension GL_EXT_nonuniform_qualifier: enable
#include "assets/shaders/library/camera.glsl"
#include "assets/shaders/library/object.glsl"

layout (location = 0) in vec3 vPosition;
layout (location = 1) in vec2 vTexCoords;
layout (location = 2) in vec3 vNormal;
layout (location = 3) in vec3 vColor;
layout (location = 4) in vec4 vTangent;

layout (location = 0) out vec3 outColor;
layout (location = 1) out vec2 outTexCoords;
layout (location = 2) out vec3 outNormal;
layout (location = 3) out vec3 outWorldPos;
layout (location = 4) out mat3 outTBN;
layout (location = 7) out vec4 outShadowCoord;

layout( push_constant ) uniform constants
{
	ivec4 handles;
} pushConstants;

const mat4 biasMat = mat4(
0.5, 0.0, 0.0, 0.0,
0.0, 0.5, 0.0, 0.0,
0.0, 0.0, 1.0, 0.0,
0.5, 0.5, 0.0, 1.0 );

void main()
{
	mat4 modelMatrix = modelData.models[pushConstants.handles.x].model;
	mat3 normalMatrix = mat3(modelData.models[pushConstants.handles.x].normal);
	vec3 worldPos = vec3(modelMatrix * vec4(vPosition, 1.0f));
	outWorldPos = worldPos;
	outShadowCoord = biasMat * cameraData.sunProj * cameraData.sunView * vec4(worldPos, 1.0f);
	outColor = vColor;
	outTexCoords = vTexCoords;
	outNormal = normalMatrix * vNormal;

	vec3 T = normalize(vec3(normalMatrix * vec3(vTangent.xyz)));
	vec3 N = normalize(vec3(normalMatrix * vec3(vNormal)));
	// re-orthogonalize T with respect to N
	T = normalize(T - dot(T, N) * N);
	// then retrieve perpendicular vector B with the cross product of T and N
	vec3 B = cross(N, T) * vTangent.w;
	outTBN = mat3(T, B, N);

	gl_Position = cameraData.proj * cameraData.view * modelMatrix * vec4(vPosition, 1.0f);
}