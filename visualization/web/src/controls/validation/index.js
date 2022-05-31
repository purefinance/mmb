function parseError(e) {
    try {
        const errorData = JSON.parse(e.response.data.error);
        const errors = [];
        if (errorData.errorFields)
            errorData.errorFields.forEach((e) => {
                errors[e.PropertyName] = e.ErrorMessage;
            });
        return {error: errorData.message, errorFields: errors};
    } catch (er) {
        return {error: e.response.data.error};
    }
}

function hasError(errorFields) {
    let error = false;
    for (const key in errorFields) {
        if (errorFields[key] !== "") error = true;
    }

    return error;
}

export {parseError, hasError};
