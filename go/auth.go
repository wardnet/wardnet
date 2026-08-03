package wardnet

import (
	"context"
	"time"

	"wardnet.network/go/internal/rest"
)

// AuthService exchanges admin credentials for a session token.
type AuthService struct{ c *Client }

// Session is a freshly issued admin session.
type Session struct {
	// Token is the bearer credential to pass to [WithToken] for subsequent
	// requests.
	Token string `json:"token"`
	// ExpiresIn is the token's remaining lifetime from the moment it was
	// issued.
	ExpiresIn time.Duration `json:"expires_in"`
}

// Login authenticates with a username and password and returns a session
// token. The returned token can be supplied to [WithToken] when constructing a
// [Client].
func (s *AuthService) Login(ctx context.Context, username, password string) (*Session, error) {
	resp, err := s.c.rest.LoginWithResponse(ctx, rest.LoginJSONRequestBody{
		Username: username,
		Password: password,
	})
	if err != nil {
		return nil, err
	}
	if resp.JSON200 == nil {
		return nil, apiError(resp.HTTPResponse, resp.Body)
	}
	return &Session{
		Token:     resp.JSON200.Token,
		ExpiresIn: time.Duration(resp.JSON200.ExpiresInSeconds) * time.Second,
	}, nil
}
