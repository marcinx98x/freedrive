package handlers

import (
	"encoding/json"
	"net/http"

	"github.com/abdullaabdullazade/freedrive/internal/api/middleware"
	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/abdullaabdullazade/freedrive/internal/service"
	"github.com/go-chi/chi/v5"
)

// ComputerHandler handles computer endpoints.
type ComputerHandler struct {
	computerService *service.ComputerService
}

// NewComputerHandler creates a new computer handler.
func NewComputerHandler(computerService *service.ComputerService) *ComputerHandler {
	return &ComputerHandler{computerService: computerService}
}

// List handles GET /api/v1/computers
func (h *ComputerHandler) List(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())

	computers, err := h.computerService.List(r.Context(), userID)
	if err != nil {
		writeError(w, err.Error(), http.StatusInternalServerError)
		return
	}
	if computers == nil {
		computers = []domain.Computer{}
	}

	writeJSON(w, http.StatusOK, map[string]interface{}{"computers": computers})
}

// Get handles GET /api/v1/computers/{id}
func (h *ComputerHandler) Get(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())
	computerID := chi.URLParam(r, "id")

	computer, err := h.computerService.Get(r.Context(), userID, computerID)
	if err != nil {
		writeError(w, err.Error(), http.StatusNotFound)
		return
	}

	writeJSON(w, http.StatusOK, computer)
}

// Register handles POST /api/v1/computers/register
func (h *ComputerHandler) Register(w http.ResponseWriter, r *http.Request) {
	userID := middleware.GetUserID(r.Context())

	var req struct {
		Name     string `json:"name"`
		Hostname string `json:"hostname"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, "invalid request body", http.StatusBadRequest)
		return
	}

	computer, err := h.computerService.Register(r.Context(), userID, req.Name, req.Hostname)
	if err != nil {
		writeError(w, err.Error(), http.StatusBadRequest)
		return
	}

	writeJSON(w, http.StatusCreated, computer)
}
