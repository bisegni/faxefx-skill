class AxeError(Exception):
    def __init__(self, code, message, **details):
        super().__init__(message)
        self.code = code
        self.details = details


def require(condition, code, message, **details):
    if not condition:
        raise AxeError(code, message, **details)

