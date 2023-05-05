#extension GL_EXT_nonuniform_qualifier: enable

layout (set = 0, binding = 0) uniform sampler samplers[];
layout (set = 0, binding = 1) uniform texture2D bindlessTextures[];
layout (set = 0, binding = 1) uniform textureCube bindlessCubeTextures[];

vec4 SampleBindlessTexture(int samplerHandle, int handle, vec2 texCoords)
{
    vec4 result = vec4(0);
    if (handle > 0){
        result = texture(sampler2D(bindlessTextures[nonuniformEXT(handle - 1)], samplers[nonuniformEXT(samplerHandle)]), texCoords);
    }
    return result;
}

vec4 SampleBindlessSkybox(int samplerHandle, int handle, vec3 viewDir)
{
    vec4 result = vec4(0);
    if (handle > 0){
        result.rgb = texture(samplerCube(bindlessCubeTextures[nonuniformEXT(handle - 1)], samplers[nonuniformEXT(samplerHandle)]), viewDir).rgb;
    }
    return result;
}