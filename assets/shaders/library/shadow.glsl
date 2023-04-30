float ShadowCalculation(sampler2DShadow shadowMap, vec4 projCoords)
{
    float closestDepth = texture(shadowMap, projCoords.xyz).r;
    float currentDepth = projCoords.z;
    float bias = 0.001;
    float ambient = 0.0;
    float shadow = 0.0;
    vec2 texelSize = 1.0 / textureSize(shadowMap, 0);
    int offset = 1;
    for(int x = -offset; x <= offset; ++x)
    {
        for(int y = -offset; y <= offset; ++y)
        {
            float pcfDepth = texture(shadowMap, vec3(projCoords.xy + vec2(x, y) * texelSize, projCoords.z));
            shadow += currentDepth - bias > pcfDepth  ? 1.0 - ambient : 0.0;
        }
    }
    int offsetDivide = ((offset * 2) + 1) * ((offset * 2) + 1);
    shadow /= float(offsetDivide);

    if (projCoords.z > 1.0) {
        return 0.0;
    }

    return shadow;
}